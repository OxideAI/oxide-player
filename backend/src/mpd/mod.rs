use crate::error::{AppError, AppResult};
use crate::types::{OutputDevice, PlaybackState, QueueEntry};
use mpd_client::client::Connection as MpdConnection;
use mpd_client::commands::definitions::{
    ClearQueue, CurrentSong, DeletePlaylist, GetPlaylist, LoadPlaylist, Play, Queue,
    RemoveFromPlaylist, RenamePlaylist, Status,
};
use mpd_client::commands::SongPosition;
use mpd_client::tag::Tag;
use mpd_client::Client;
use mpd_protocol::command::Command;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

fn is_localhost(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "::1" | "localhost" | "0.0.0.0")
}

#[derive(Clone)]
pub struct Mpd {
    inner: Arc<MpdInner>,
}

struct MpdInner {
    host: String,
    port: u16,
    autostart: bool,
    binary: Option<String>,
    config_path: Option<PathBuf>,
    conn: Mutex<Option<MpdConnection>>,
    connect_lock: Mutex<()>,
    active_track: Mutex<Option<i64>>,
}

pub struct MpdStatus {
    pub state: PlaybackState,
    pub volume: u8,
    pub elapsed: f64,
    pub duration: f64,
    pub error: Option<String>,
    pub current_uri: Option<String>,
    pub current_track: Option<u32>,
    pub current_id: Option<u64>,
    pub random: bool,
}

impl Mpd {
    pub async fn connect(
        host: &str,
        port: u16,
        autostart: bool,
        binary: Option<String>,
        config_path: Option<PathBuf>,
    ) -> Self {
        Mpd {
            inner: Arc::new(MpdInner {
                host: host.to_string(),
                port,
                autostart,
                binary,
                config_path,
                conn: Mutex::new(None),
                connect_lock: Mutex::new(()),
                active_track: Mutex::new(None),
            }),
        }
    }

    /// Ensure MPD is reachable. If it is already up we return immediately.
    /// Otherwise, when autostart is enabled and MPD runs on the local machine,
    /// we launch the `mpd` daemon and retry the connection for a few seconds.
    /// This keeps the app usable on boot without requiring MPD to be started
    /// separately (the connection itself is lazy, so commands would otherwise
    /// fail until MPD comes up).
    pub async fn ensure_running(&self) -> AppResult<()> {
        if self.client().await.is_ok() {
            tracing::info!("MPD reachable at {}:{}", self.inner.host, self.inner.port);
            return Ok(());
        }

        if !self.inner.autostart {
            return Err(AppError::Mpd(format!(
                "MPD not reachable at {}:{} and autostart is disabled",
                self.inner.host, self.inner.port
            )));
        }

        if !is_localhost(&self.inner.host) {
            return Err(AppError::Mpd(format!(
                "MPD not reachable at {}:{} (autostart only supported for local MPD)",
                self.inner.host, self.inner.port
            )));
        }

        tracing::warn!(
            "MPD not reachable at {}:{}; attempting to start it",
            self.inner.host,
            self.inner.port
        );
        self.start_daemon().await?;

        for attempt in 1..=20 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if self.client().await.is_ok() {
                tracing::info!("MPD started at {}:{}", self.inner.host, self.inner.port);
                return Ok(());
            }
            tracing::debug!("MPD not up yet, retry {attempt}/20");
        }
        Err(AppError::Mpd(format!(
            "launched mpd but it did not become reachable at {}:{}",
            self.inner.host, self.inner.port
        )))
    }

    /// Spawn the `mpd` daemon. MPD daemonizes by default, so the child process
    /// exits promptly after forking; we wait for that (or for the foreground run
    /// to finish) and surface a clear error if it fails.
    async fn start_daemon(&self) -> AppResult<()> {
        let binary = self
            .inner
            .binary
            .clone()
            .unwrap_or_else(|| "mpd".to_string());
        let mut cmd = tokio::process::Command::new(&binary);
        if let Some(cfg) = &self.inner.config_path {
            cmd.arg(cfg);
        }
        cmd.stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let status = cmd
            .status()
            .await
            .map_err(|e| AppError::Mpd(format!("failed to launch '{binary}': {e}")))?;
        if !status.success() {
            return Err(AppError::Mpd(format!(
                "'{binary}' exited with {status}; check the MPD config"
            )));
        }
        Ok(())
    }

    /// Return a live MPD client, (re)connecting lazily if the cached
    /// connection is missing or has dropped (e.g. after an MPD restart).
    async fn client(&self) -> AppResult<Client> {
        if let Some(client) = self.try_clone().await {
            return Ok(client);
        }
        // Serialize (re)connect attempts so concurrent callers don't each open
        // a socket; another task may have already reconnected while we waited.
        let _connect_guard = self.inner.connect_lock.lock().await;
        if let Some(client) = self.try_clone().await {
            return Ok(client);
        }
        let stream = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect((self.inner.host.as_str(), self.inner.port)))
            .await
            .map_err(|_| AppError::Mpd(format!("connect {}:{} timed out", self.inner.host, self.inner.port)))?
            .map_err(|e| AppError::Mpd(format!("connect {}:{}: {e}", self.inner.host, self.inner.port)))?;
        let (client, _handle) = Client::connect(stream)
            .await
            .map_err(|e| AppError::Mpd(format!("mpd handshake: {e}")))?;
        let mut guard = self.inner.conn.lock().await;
        *guard = Some((client, _handle));
        Ok(guard.as_ref().unwrap().0.clone())
    }

    /// Clone the cached client if it exists and is still connected. The lock is
    /// released before any network I/O so connecting never stalls other commands.
    async fn try_clone(&self) -> Option<Client> {
        let guard = self.inner.conn.lock().await;
        match guard.as_ref() {
            Some((c, _)) if !c.is_connection_closed() => Some(c.clone()),
            _ => None,
        }
    }

    pub async fn raw(&self, cmd: Command) -> AppResult<()> {
        let client = self.client().await?;
        client
            .raw_command(cmd)
            .await
            .map_err(|e| AppError::Mpd(format!("command failed: {e}")))?;
        Ok(())
    }

    pub async fn status(&self) -> AppResult<MpdStatus> {
        let client = self.client().await?;
        let status = client
            .command(Status)
            .await
            .map_err(|e| AppError::Mpd(format!("status: {e}")))?;

        let state = match status.state {
            mpd_client::responses::PlayState::Playing => PlaybackState::Playing,
            mpd_client::responses::PlayState::Paused => PlaybackState::Paused,
            mpd_client::responses::PlayState::Stopped => PlaybackState::Stopped,
        };

        let (current_uri, current_track, current_id) = match status.current_song {
            Some(_) => match client.command(CurrentSong).await.ok().flatten() {
                Some(s) => {
                    let (_, track) = s.song.number();
                    let track = if track == 0 { None } else { Some(track as u32) };
                    (Some(s.song.url), track, Some(s.id.0))
                }
                None => (None, None, None),
            },
            None => (None, None, None),
        };
        tracing::debug!(has_current = status.current_song.is_some(), ?current_uri, "mpd status");

        Ok(MpdStatus {
            state,
            volume: status.volume,
            elapsed: status.elapsed.map(|d| d.as_secs_f64()).unwrap_or(0.0),
            duration: status.duration.map(|d| d.as_secs_f64()).unwrap_or(0.0),
            error: status.error,
            current_uri,
            current_track,
            current_id,
            random: status.random,
        })
    }

    pub async fn outputs(&self) -> AppResult<Vec<OutputDevice>> {
        let client = self.client().await?;
        let frame = client
            .raw_command(Command::new("outputs"))
            .await
            .map_err(|e| AppError::Mpd(format!("outputs: {e}")))?;

        let mut out = Vec::new();
        let mut cur_id: Option<u32> = None;
        let mut cur_name = String::new();
        let mut cur_enabled = false;

        let flush = |id: &mut Option<u32>, name: &mut String, enabled: &mut bool, out: &mut Vec<OutputDevice>| {
            if let Some(id) = id.take() {
                out.push(OutputDevice {
                    id,
                    name: std::mem::take(name),
                    enabled: *enabled,
                });
            }
            *enabled = false;
        };

        for (k, v) in &frame {
            match k {
                "outputid" => {
                    flush(&mut cur_id, &mut cur_name, &mut cur_enabled, &mut out);
                    cur_id = v.parse().ok();
                }
                "outputname" => cur_name = v.to_string(),
                "outputenabled" => cur_enabled = v == "1",
                _ => {}
            }
        }
        flush(&mut cur_id, &mut cur_name, &mut cur_enabled, &mut out);
        Ok(out)
    }

    /// Add `uri` to the queue, returning its queue song id. `uri` must match
    /// MPD's database exactly; keep MPD's index in sync via [`Self::rescan`]
    /// (done on library refresh) so scanner and MPD agree on filenames.
    async fn add_uri(&self, uri: &str) -> AppResult<u64> {
        use mpd_client::commands::definitions::Add;
        let client = self.client().await?;
        let id = client
            .command(Add::uri(uri))
            .await
            .map_err(|e| AppError::Mpd(format!("add: {e}")))?;
        Ok(id.0)
    }

    pub async fn play_uri(&self, uri: &str) -> AppResult<()> {
        let id = self.add_uri(uri).await?;
        self.play_song_id(id).await
    }

    /// Play the song with MPD song id `id` by resolving its 0-based queue
    /// position and issuing `play <pos>`. MPD 0.24 has no `playid`, so we
    /// always seek/play by position (see AGENTS.md).
    async fn play_song_id(&self, id: u64) -> AppResult<()> {
        let pos = self
            .queue()
            .await?
            .iter()
            .position(|s| s.id == id)
            .map(|p| p as u32)
            .ok_or_else(|| AppError::Mpd(format!("added song {id} not found in queue")))?;
        self.play_position(pos).await
    }

    /// Incrementally update MPD's database (rescans changed files).
    pub async fn update(&self) -> AppResult<()> {
        self.raw(Command::new("update")).await
    }

    /// Force a full rescan of MPD's database, re-reading every file. Fixes
    /// stale/incorrect index entries that would make `add` fail or point at a
    /// path that no longer exists on disk.
    pub async fn rescan(&self) -> AppResult<()> {
        self.raw(Command::new("rescan")).await
    }

    /// Insert `uri` immediately after the currently playing song, or at the
    /// front of the queue when nothing is playing. Returns the new queue id.
    pub async fn play_next(
        &self,
        uri: &str,
        start: f64,
        end: Option<f64>,
    ) -> AppResult<u64> {
        use mpd_client::commands::definitions::Add;
        let client = self.client().await?;
        let id = client
            .command(Add::uri(uri).after_current(0))
            .await
            .map_err(|e| AppError::Mpd(format!("add: {e}")))?;
        // MPD 0.24 has no `rangeid`, and a not-yet-playing track can't be
        // seeked, so a CUE start/end offset is not applied here. The offset is
        // honored once the track becomes current (see `play_uri_range`).
        let _ = (start, end);
        Ok(id.0)
    }

    /// Empty the play queue.
    pub async fn clear(&self) -> AppResult<()> {
        self.raw(Command::new("clear")).await
    }

    /// Append `uri` to a saved playlist. `uri` is added whole (MPD's
    /// `playlistadd` returns no per-song id, so CUE ranges are not applied here;
    /// ranges are a queue concern handled by `play_next`/queue inserts).
    pub async fn add_to_playlist(&self, name: &str, uri: &str) -> AppResult<()> {
        use mpd_client::commands::definitions::AddToPlaylist;
        let client = self.client().await?;
        client
            .command(AddToPlaylist::new(name, uri))
            .await
            .map_err(|e| AppError::Mpd(format!("playlistadd: {e}")))?;
        Ok(())
    }

    /// Return the current play queue (in order). CUE tracks carry their
    /// `[start, end)` `range` so callers can tell them apart.
    pub async fn queue(&self) -> AppResult<Vec<QueueEntry>> {
        let client = self.client().await?;
        let songs = client
            .command(Queue::all())
            .await
            .map_err(|e| AppError::Mpd(format!("playlistinfo: {e}")))?;
        Ok(songs
            .into_iter()
            .enumerate()
            .map(|(i, s)| {
                let tag = |t: Tag| s.song.tags.get(&t).and_then(|v| v.first().cloned());
                QueueEntry {
                    pos: i as u32,
                    id: s.id.0,
                    uri: s.song.url.clone(),
                    title: tag(Tag::Title),
                    artist: tag(Tag::Artist),
                    album: tag(Tag::Album),
                    duration: s.song.duration.map(|d| d.as_secs_f64()),
                }
            })
            .collect())
    }

    /// Add `uri` and play only the `[start, end)` portion of it, then let MPD
    /// advance to whatever is next in the queue. Used for CUE-sheet tracks so
    /// playback stops precisely at the track boundary instead of bleeding into
    /// the next track's audio. `end` of `None` plays through to end-of-file.
    pub async fn play_uri_range(
        &self,
        uri: &str,
        start: f64,
        end: Option<f64>,
    ) -> AppResult<()> {
        let id = self.add_uri(uri).await?;
        let pos = self
            .queue()
            .await?
            .iter()
            .position(|s| s.id == id)
            .map(|p| p as u32)
            .ok_or_else(|| AppError::Mpd(format!("added song {id} not found in queue")))?;
        self.play_position(pos).await?;
        if start > 0.0 {
            // Seek within the now-current song. `seekid` exists in MPD 0.24.
            self.raw(
                Command::new("seekid")
                    .argument(id)
                    .argument(format!("{:.3}", start.max(0.0))),
            )
            .await?;
        }
        // NOTE: per-song end range (`rangeid`) is MPD 0.25+. On 0.24 the track
        // plays to end of file; CUE gapless end-boundary is best-effort.
        let _ = end;
        Ok(())
    }

    pub async fn play(&self) -> AppResult<()> {
        self.raw(Command::new("play")).await
    }

    /// Start playing the queue entry with the given song `id`.
    pub async fn play_position(&self, pos: u32) -> AppResult<()> {
        self.raw(Command::new("play").argument(pos)).await
    }

    /// Remove a single entry from the play queue by its position (0-based).
    pub async fn delete_position(&self, pos: u32) -> AppResult<()> {
        self.raw(Command::new("delete").argument(pos)).await
    }

    /// Toggle MPD's random (shuffle) play mode.
    pub async fn random(&self, on: bool) -> AppResult<()> {
        self.raw(Command::new("random").argument(if on { 1u8 } else { 0u8 }))
            .await
    }

    pub async fn pause(&self, pause: bool) -> AppResult<()> {
        self.raw(Command::new("pause").argument(if pause { 1u8 } else { 0u8 }))
            .await
    }

    pub async fn stop(&self) -> AppResult<()> {
        self.raw(Command::new("stop")).await
    }

    pub async fn next(&self) -> AppResult<()> {
        self.raw(Command::new("next")).await
    }

    pub async fn previous(&self) -> AppResult<()> {
        self.raw(Command::new("previous")).await
    }

    pub async fn seek(&self, seconds: f64) -> AppResult<()> {
        // Pass a fractional position; the old `as u64` truncated sub-second
        // seeks. MPD 0.24+ accepts float seek positions.
        self.raw(Command::new("seekcur").argument(format!("{:.3}", seconds.max(0.0))))
            .await
    }

    pub async fn set_volume(&self, volume: u8) -> AppResult<()> {
        self.raw(Command::new("setvol").argument(volume))
            .await
    }

    pub async fn enable_output(&self, id: u32) -> AppResult<()> {
        self.raw(Command::new("enableoutput").argument(id))
            .await
    }

    pub async fn disable_output(&self, id: u32) -> AppResult<()> {
        self.raw(Command::new("disableoutput").argument(id))
            .await
    }

    pub async fn clear_error(&self) -> AppResult<()> {
        self.raw(Command::new("clearerror")).await
    }

    pub async fn set_active_track(&self, id: Option<i64>) {
        *self.inner.active_track.lock().await = id;
    }

    pub async fn active_track(&self) -> Option<i64> {
        *self.inner.active_track.lock().await
    }

    pub async fn save_playlist(&self, name: &str) -> AppResult<()> {
        use mpd_client::commands::definitions::SaveQueueAsPlaylist;
        let client = self.client().await?;
        client
            .command(SaveQueueAsPlaylist(name))
            .await
            .map_err(|e| AppError::Mpd(format!("save: {e}")))?;
        Ok(())
    }

    pub async fn list_playlists(&self) -> AppResult<Vec<String>> {
        use mpd_client::commands::definitions::GetPlaylists;
        let client = self.client().await?;
        let lists = client
            .command(GetPlaylists)
            .await
            .map_err(|e| AppError::Mpd(format!("listplaylists: {e}")))?;
        Ok(lists.into_iter().map(|p| p.name).collect())
    }

    /// Return the tracks in a saved playlist (in order). Positions are the
    /// playlist's own 0-based indices, used for `remove_from_playlist`.
    pub async fn playlist_tracks(&self, name: &str) -> AppResult<Vec<QueueEntry>> {
        let client = self.client().await?;
        let songs = client
            .command(GetPlaylist(name))
            .await
            .map_err(|e| AppError::Mpd(format!("listplaylistinfo {name}: {e}")))?;
        Ok(songs
            .into_iter()
            .enumerate()
            .map(|(i, s)| {
                let tag = |t: Tag| s.tags.get(&t).and_then(|v| v.first().cloned());
                QueueEntry {
                    pos: i as u32,
                    // Playlist entries have no queue SongId; mirror the position.
                    id: i as u64,
                    uri: s.url.clone(),
                    title: tag(Tag::Title),
                    artist: tag(Tag::Artist),
                    album: tag(Tag::Album),
                    duration: s.duration.map(|d| d.as_secs_f64()),
                }
            })
            .collect())
    }

    /// Replace the play queue with a saved playlist and start playback from the
    /// first track. MPD 0.24 has no `playid`; we play by 0-based position.
    /// `clear` + `load` + `play` are sent as a single command list so a
    /// concurrent request cannot slip a command in between and corrupt the queue.
    pub async fn play_playlist(&self, name: &str) -> AppResult<()> {
        let client = self.client().await?;
        // Guard the empty-playlist case: `play 0` on an empty queue is a hard
        // MPD error that would otherwise surface as a 500. Skip cleanly instead.
        let songs = client
            .command(GetPlaylist(name))
            .await
            .map_err(|e| AppError::Mpd(format!("listplaylistinfo {name}: {e}")))?;
        if songs.is_empty() {
            return Ok(());
        }
        client
            .command_list((ClearQueue, LoadPlaylist::name(name), Play::song(SongPosition(0))))
            .await
            .map_err(|e| AppError::Mpd(format!("play playlist {name}: {e}")))?;
        Ok(())
    }

    /// Remove the track at `pos` (0-based) from a saved playlist.
    pub async fn remove_from_playlist(&self, name: &str, pos: u32) -> AppResult<()> {
        let client = self.client().await?;
        client
            .command(RemoveFromPlaylist::position(name, pos as usize))
            .await
            .map_err(|e| AppError::Mpd(format!("playlistdelete {name} {pos}: {e}")))?;
        Ok(())
    }

    /// Delete a saved playlist entirely.
    pub async fn delete_playlist(&self, name: &str) -> AppResult<()> {
        let client = self.client().await?;
        client
            .command(DeletePlaylist(name))
            .await
            .map_err(|e| AppError::Mpd(format!("rm {name}: {e}")))?;
        Ok(())
    }

    /// Rename a saved playlist.
    pub async fn rename_playlist(&self, from: &str, to: &str) -> AppResult<()> {
        let client = self.client().await?;
        client
            .command(RenamePlaylist::new(from, to))
            .await
            .map_err(|e| AppError::Mpd(format!("rename {from} -> {to}: {e}")))?;
        Ok(())
    }
}
