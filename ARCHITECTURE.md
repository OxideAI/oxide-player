# Architecture — Oxide

Oxide is a music server: a Rust backend that talks to **MPD** (playback) and **CamillaDSP** (DSP)
and serves a **React** web UI. The backend owns a SQLite library database and a cover-art cache,
and exposes a JSON HTTP API. The built frontend (`frontend/dist/`) is served as static files by the
backend itself.

```
┌────────────┐      HTTP/JSON       ┌──────────────────┐     mpd_protocol      ┌──────┐
│  Browser   │ ───────────────────▶ │  oxide-player     │ ───────────────────▶ │ MPD  │ ──▶ audio
│ (React SPA)│ ◀─────────────────── │  (Axum backend)   │                      └──────┘
└────────────┘     static dist/     └────────┬─────────┘
                                             │ SQLite (rusqlite) + covers
                                             ▼
                                    ┌──────────────────┐
                                    │ CamillaDSP (DSP) │
                                    └──────────────────┘
```

## Backend (`backend/src/`)

| Module               | Responsibility                                                                                                                                                                                                  |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `main.rs`            | `tokio` runtime, CLI parse, build router, spawn status poller, bind listener.                                                                                                                                   |
| `config.rs`          | `Cli` (clap) + `Config` (JSON, `--config`/`-c`); resolves `data_dir`, `static_dir`, MPD host/port.                                                                                                              |
| `mpd/mod.rs`         | `Mpd` connection wrapper. `status()`, `queue()`, `play_position(pos)`, `play_next`, `delete_position(pos)`, `random(on)`, `outputs()`, `rescan()`, `set_active_track()`, etc. Uses `mpd_client`/`mpd_protocol`. |
| `api/mod.rs`         | Axum `Router`: all `/api/*` routes + static file serving (`ServeDir` + SPA fallback to `index.html`).                                                                                                           |
| `state.rs`           | `AppState` (shared `Mpd`, `Db`, cached `PlayerStatus`). `refresh_status()` runs every 1s, polls MPD, updates the cached status, and prunes dead tracks on MPD errors.                                           |
| `types.rs`           | Wire types: `PlayerStatus`, `Track`, `TrackRef`, `QueueEntry`, `QueueResponse`, `QueueItem` (frontend), `DspProfile`, `OutputDevice`, etc.                                                                      |
| `library/scanner.rs` | Walk the music root, read tags with `lofty`, upsert into SQLite, extract covers.                                                                                                                                |
| `library/db.rs`      | SQLite schema, FTS5 search, CUE addressing.                                                                                                                                                                     |
| `shared_folders.rs`  | Render `<data_dir>/smb-shares.conf` (one `[share]` per `library_dirs` entry) and `smbcontrol smbd reload-config`. Called by the `/api/config/library-dirs` add/remove handlers before config persistence.       |
| `radio.rs`           | `RadioManager`: CRUD over HTTP(S) stations persisted to `<data_dir>/radio_stations.json`.                                                                                                                       |
| `visualizer/`        | cpal capture + `rustfft` (72 log-spaced bins), broadcast over `/api/visualizer` WS.                                                                                                                             |
| `bluetooth/`         | Linux-only bluer manager (`linux.rs`); `stub.rs` mirror elsewhere.                                                                                                                                              |
| `devices/`           | Per-output MPD config fragments (`mpd-outputs.d/`) + include injection.                                                                                                                                         |
| `dsp/`               | CamillaDSP profile load/validate/apply.                                                                                                                                                                         |
| `error.rs`           | `AppError` → HTTP responses.                                                                                                                                                                                    |

### Status pipeline

`state::spawn_status_poller` loops every second: `mpd.status()` → map into the cached
`PlayerStatus` (volume, elapsed, duration, `random`, `current_song`, outputs). The web UI polls
`GET /api/status` once per second, so the UI tracks playback with ~1s latency. The queue panel
additionally polls `GET /api/queue` every second while open so the highlighted row follows
next/prev.

### Library database

SQLite (`rusqlite`, bundled) at `<data_dir>/library.db`. Tracks are keyed by an integer `id`; the
`file_mtime` column exists for cache invalidation and is added idempotently on startup. Covers are
cached (decoded/extracted) under `<data_dir>/covers/<id>.<ext>`. CUE sheets: every track of a CUE
album shares one `uri`; now-playing matching is by track `id`, and cover lookup falls back to an
album-named jpg then a single image in the folder.

## Frontend (`frontend/src/`)

| File                                                           | Responsibility                                                                                                                         |
| -------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `App.tsx`                                                      | Top-level: tab routing (library/devices/dsp/playlists), status polling, wires handlers into `NowPlaying`.                              |
| `api.ts`                                                       | Typed fetch wrappers for every endpoint (`status`, `queue`, `shuffle`, `jump`, `remove`, `play`, …).                                   |
| `types.ts`                                                     | TS mirrors of backend wire types + `QueueItem`/`QueueResponse`.                                                                        |
| `util.ts`                                                      | `fmtTime`, `displayTitle`, `audioQuality`.                                                                                             |
| `components/LibraryView.tsx`                                   | Album grid + track list; row classes `row` / `rowActive` / `rowPlaying` / `rowPaused`; 3-dot `TrackMenu` per row and on album headers. |
| `components/NowPlaying.tsx`                                    | Player bar: transport, scrub, volume, **queue toggle + shuffle** buttons. Owns the open queue state.                                   |
| `components/QueueView.tsx`                                     | Flyout queue panel: list entries (pos, title, artist, duration), highlight `current`, click-to-jump, per-row remove.                   |
| `components/TrackMenu.tsx`                                     | 3-dot popover: play-next / clear-play / add-to-playlist / file-info.                                                                   |
| `components/{KioskView,DspView,DevicesView,PlaylistsView}.tsx` | Respective screens.                                                                                                                    |

Styling is **CSS Modules** (`*.module.css`) — no Tailwind. Each component gets a co-located module.

## API reference

| Method      | Path                                 | Body                               | Purpose                                                                              |
| ----------- | ------------------------------------ | ---------------------------------- | ------------------------------------------------------------------------------------ |
| GET         | `/api/status`                        | —                                  | `PlayerStatus` (state, volume, elapsed, duration, `random`, `current_song`, outputs) |
| GET         | `/api/ws`                            | —                                  | WebSocket status stream (alternative to polling `/api/status`)                       |
| GET         | `/api/library`                       | `q?` `artist?` `album?`            | Tracks matching filters                                                              |
| GET         | `/api/library/albums`                | —                                  | Album names                                                                          |
| GET         | `/api/library/albums/sources`        | —                                  | Library source roots                                                                 |
| GET         | `/api/library/artists`               | —                                  | Artist names                                                                         |
| GET         | `/api/cover/{key}`                   | —                                  | Cover image bytes (by cover key)                                                     |
| POST        | `/api/library/scan`                  | —                                  | Scan music root into DB                                                              |
| POST        | `/api/library/refresh`               | —                                  | Re-scan                                                                              |
| POST        | `/api/library/rescan-art`            | —                                  | Re-extract cover art                                                                 |
| POST        | `/api/playback/play`                 | `{ uri }`                          | Play URI                                                                             |
| POST        | `/api/playback/pause`                | —                                  | Pause/resume                                                                         |
| POST        | `/api/playback/stop`                 | —                                  | Stop                                                                                 |
| POST        | `/api/playback/next` `prev`          | —                                  | Transport                                                                            |
| POST        | `/api/playback/seek`                 | `{ seconds }`                      | Seek                                                                                 |
| POST        | `/api/playback/volume`               | `{ volume }`                       | Set volume                                                                           |
| POST        | `/api/playback/play-next`            | `{ uri, start?, end?, track_id? }` | Insert after current                                                                 |
| POST        | `/api/playback/clear-play`           | `{ uri, start?, end?, track_id? }` | Clear queue, play track                                                              |
| GET         | `/api/queue`                         | —                                  | `QueueResponse { entries, current }` (`current` = playing **position**)              |
| POST        | `/api/playback/shuffle`              | `{ on: bool }`                     | Toggle MPD `random` mode                                                             |
| POST        | `/api/playback/jump`                 | `{ pos: u32 }`                     | Jump to queue position (0-based)                                                     |
| POST        | `/api/playback/remove`               | `{ pos: u32 }`                     | Remove queue entry by position                                                       |
| POST        | `/api/playback/clear-queue`          | —                                  | Clear the whole queue                                                                |
| GET         | `/api/devices`                       | —                                  | MPD outputs                                                                          |
| POST        | `/api/devices/{id}/enable` `disable` | —                                  | Toggle output                                                                        |
| GET/PUT     | `/api/dsp`                           | `DspProfile`                       | Read/apply CamillaDSP profile                                                        |
| GET         | `/api/playlists`                     | —                                  | Playlist names                                                                       |
| POST        | `/api/playlists`                     | `{ name, profile? }`               | Save playlist                                                                        |
| POST        | `/api/playlists/{name}/add`          | `{ tracks }`                       | Add track(s) to a playlist                                                           |
| GET         | `/api/playlists/{name}`              | —                                  | Tracks in a playlist                                                                 |
| POST        | `/api/playlists/{name}/play`         | —                                  | Play a playlist                                                                      |
| POST        | `/api/playlists/{name}/remove`       | `{ uris }`                         | Remove track(s) from a playlist                                                      |
| POST        | `/api/playlists/{name}/rename`       | `{ name }`                         | Rename a playlist                                                                    |
| DELETE      | `/api/playlists/{name}`              | —                                  | Delete a playlist                                                                    |
| GET/PUT     | `/api/config`                        | `Config`                           | Read/update runtime config                                                           |
| POST/DELETE | `/api/config/library-dirs`           | `{ path }`                         | Add/remove a library source dir                                                      |

## MPD integration notes (gotchas)

- Smoke MPD is **0.24.0**. The **`playid` command does not exist** there → seek/play by **0-based
  position**: `play <pos>`. `Mpd::play_position(pos)` does this. Do not reintroduce `playid`.
- **Shuffle** is MPD's `random` mode (`random 0|1`), not a one-shot reorder. `PlayerStatus.random`
  carries the state.
- **Remove** is `delete <pos>`.
- `QueueEntry.id` is the MPD **SongId**; it does **not** equal `PlayerStatus.current_song.id` (the
  DB track id). Never compare the two.
- CUE albums share one `uri`; highlight by track `id`.
- **MPD 0.24 rejects local-file URIs over TCP** (`add file://...` → ACK 4 "Access to local files
  via TCP is not allowed"): the peer uid is only knowable on an AF_UNIX socket. The backend
  therefore prefers `/run/mpd/socket` (`Mpd::client` falls back to TCP only when the socket is
  absent), and the installer binds it in the managed `mpd.conf` — next to the TCP
  `127.0.0.1:6600` bind — plus repairs existing configs via `ensure_mpd_include`.

## Installer & system integration (`install.sh`)

Idempotent installer for Debian hosts. Modes: full install (default), `--update` (binary +
frontend + reinstall/repair services), `--fix-perms`, `--uninstall`. Env overrides control
paths (`DATA_DIR`, `MPD_MUSIC_DIR`, …), the listen address (`LISTEN`), and tuning
(`WIFI_IFACE`, `WIFI_HEAL_INTERVAL`, `KIOSK_ENABLED`). Highlights:

- Music library: `MPD_MUSIC_DIR` is created, FAT-family mounts get group-writable mount masks
  (`fmask=0113,dmask=0002`, so the `audio` group can scp), ownership is normalized to the
  service user.
- Samba: regenerates `/etc/samba/smb.conf` with an `include` of
  `<data_dir>/smb-shares.conf`; `deadtime = 120` keeps idle SMB sessions (Finder) alive through
  network blips. Update mode adds the deadtime to pre-existing Oxide-managed configs
  (`ensure_samba_deadtime`); user-managed configs are untouched.
- Library sources: the backend rewrites `smb-shares.conf` on
  `POST/DELETE /api/config/library-dirs`; the installer only bootstraps the initial share.
- Wall kiosk (cage + Chromium) is opt-in via `KIOSK_ENABLED=1`.
- Avahi advertises the web UI, the Music share, and the AirPlay receiver.
- Sudoers: `SUDOERS_RULE` grants the service user passwordless `systemctl reboot/poweroff` so
  the web UI power buttons work.

### WiFi stability suite

Rationale: on `brcmsmac`/`b43` Broadcom wifi, machines serving SMB over wifi have shown bursts
of `CTRL-EVENT-BEACON-LOSS` and silent link death (no journal error; the box just vanishes from
the LAN). The installer provisions two mitigations plus a wired precedence rule:

1. `oxide-wifi-powersave-off.service` — one-shot `iw dev <iface> set power_save off`, live and
   at every boot (power-save idle timers are the usual beacon-loss trigger). `ExecStart` is
   prefixed `-`, so a missing/uncooperative driver does not wedge the boot.
2. `oxide-wifi-heal.timer` + `oxide-wifi-heal.service` →
   `$BIN_DIR/oxide-wifi-heal.sh` — every `WIFI_HEAL_INTERVAL` (default 2 min): ping the
   gateway; on failure restart `wpa_supplicant` + `systemd-networkd`, then bounce the interface.
   Healthy links are a verified no-op (script exits 0). Requires `iputils-ping` (provisioned by
   `ensure_ping`; minimized Ubuntu images sometimes ship without it).
3. `/etc/netplan/01-oxide-wired-priority.yaml` — ethernet-only DHCP profile with
   `route-metric: 100`; wifi always has the higher metric, so a plugged cable wins the default
   route automatically. Removed by `--uninstall` (with a manual `netplan apply` hint).

Troubleshooting: `journalctl -t oxide-wifi-heal` shows every watchdog decision. Both services
are removed by `do_uninstall`.

## Build & serve

- Backend: `cd backend && cargo build` → `target/debug/oxide-player`; `cargo test` runs unit tests
  (`library/scanner.rs`, `dsp/config.rs`).
- Frontend: `cd frontend && npm run build` runs `tsc` (type-check) then `vite build` to `dist/`.
- The backend serves `dist/` statically; UI-only changes need no backend restart. New API routes
  require a backend rebuild + restart.
