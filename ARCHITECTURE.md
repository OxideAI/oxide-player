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

| Module | Responsibility |
| --- | --- |
| `main.rs` | `tokio` runtime, CLI parse, build router, spawn status poller, bind listener. |
| `config.rs` | `Cli` (clap) + `Config` (JSON, `--config`/`-c`); resolves `data_dir`, `static_dir`, MPD host/port. |
| `mpd/mod.rs` | `Mpd` connection wrapper. `status()`, `queue()`, `play_position(pos)`, `play_next`, `delete_position(pos)`, `random(on)`, `outputs()`, `rescan()`, `set_active_track()`, etc. Uses `mpd_client`/`mpd_protocol`. |
| `api/mod.rs` | Axum `Router`: all `/api/*` routes + static file serving (`ServeDir` + SPA fallback to `index.html`). |
| `state.rs` | `AppState` (shared `Mpd`, `Db`, cached `PlayerStatus`). `refresh_status()` runs every 1s, polls MPD, updates the cached status, and prunes dead tracks on MPD errors. |
| `types.rs` | Wire types: `PlayerStatus`, `Track`, `TrackRef`, `QueueEntry`, `QueueResponse`, `QueueItem` (frontend), `DspProfile`, `OutputDevice`, etc. |
| `library/scanner.rs` | Walk the music root, read tags with `lofty`, upsert into SQLite, extract covers. |
| `dsp/` | CamillaDSP profile load/validate/apply. |
| `error.rs` | `AppError` → HTTP responses. |

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

| File | Responsibility |
| --- | --- |
| `App.tsx` | Top-level: tab routing (library/devices/dsp/playlists), status polling, wires handlers into `NowPlaying`. |
| `api.ts` | Typed fetch wrappers for every endpoint (`status`, `queue`, `shuffle`, `jump`, `remove`, `play`, …). |
| `types.ts` | TS mirrors of backend wire types + `QueueItem`/`QueueResponse`. |
| `util.ts` | `fmtTime`, `displayTitle`, `audioQuality`. |
| `components/LibraryView.tsx` | Album grid + track list; row classes `row` / `rowActive` / `rowPlaying` / `rowPaused`; 3-dot `TrackMenu` per row and on album headers. |
| `components/NowPlaying.tsx` | Player bar: transport, scrub, volume, **queue toggle + shuffle** buttons. Owns the open queue state. |
| `components/QueueView.tsx` | Flyout queue panel: list entries (pos, title, artist, duration), highlight `current`, click-to-jump, per-row remove. |
| `components/TrackMenu.tsx` | 3-dot popover: play-next / clear-play / add-to-playlist / file-info. |
| `components/{KioskView,DspView,DevicesView,PlaylistsView}.tsx` | Respective screens. |

Styling is **CSS Modules** (`*.module.css`) — no Tailwind. Each component gets a co-located module.

## API reference

| Method | Path | Body | Purpose |
| --- | --- | --- | --- |
| GET | `/api/status` | — | `PlayerStatus` (state, volume, elapsed, duration, `random`, `current_song`, outputs) |
| GET | `/api/ws` | — | WebSocket status stream (alternative to polling `/api/status`) |
| GET | `/api/library` | `q?` `artist?` `album?` | Tracks matching filters |
| GET | `/api/library/albums` | — | Album names |
| GET | `/api/library/albums/sources` | — | Library source roots |
| GET | `/api/library/artists` | — | Artist names |
| GET | `/api/cover/{key}` | — | Cover image bytes (by cover key) |
| POST | `/api/library/scan` | — | Scan music root into DB |
| POST | `/api/library/refresh` | — | Re-scan |
| POST | `/api/library/rescan-art` | — | Re-extract cover art |
| POST | `/api/playback/play` | `{ uri }` | Play URI |
| POST | `/api/playback/pause` | — | Pause/resume |
| POST | `/api/playback/stop` | — | Stop |
| POST | `/api/playback/next` `prev` | — | Transport |
| POST | `/api/playback/seek` | `{ seconds }` | Seek |
| POST | `/api/playback/volume` | `{ volume }` | Set volume |
| POST | `/api/playback/play-next` | `{ uri, start?, end?, track_id? }` | Insert after current |
| POST | `/api/playback/clear-play` | `{ uri, start?, end?, track_id? }` | Clear queue, play track |
| GET | `/api/queue` | — | `QueueResponse { entries, current }` (`current` = playing **position**) |
| POST | `/api/playback/shuffle` | `{ on: bool }` | Toggle MPD `random` mode |
| POST | `/api/playback/jump` | `{ pos: u32 }` | Jump to queue position (0-based) |
| POST | `/api/playback/remove` | `{ pos: u32 }` | Remove queue entry by position |
| POST | `/api/playback/clear-queue` | — | Clear the whole queue |
| GET | `/api/devices` | — | MPD outputs |
| POST | `/api/devices/{id}/enable` `disable` | — | Toggle output |
| GET/PUT | `/api/dsp` | `DspProfile` | Read/apply CamillaDSP profile |
| GET | `/api/playlists` | — | Playlist names |
| POST | `/api/playlists` | `{ name, profile? }` | Save playlist |
| POST | `/api/playlists/{name}/add` | `{ tracks }` | Add track(s) to a playlist |
| GET | `/api/playlists/{name}` | — | Tracks in a playlist |
| POST | `/api/playlists/{name}/play` | — | Play a playlist |
| POST | `/api/playlists/{name}/remove` | `{ uris }` | Remove track(s) from a playlist |
| POST | `/api/playlists/{name}/rename` | `{ name }` | Rename a playlist |
| DELETE | `/api/playlists/{name}` | — | Delete a playlist |
| GET/PUT | `/api/config` | `Config` | Read/update runtime config |
| POST/DELETE | `/api/config/library-dirs` | `{ path }` | Add/remove a library source dir |

## MPD integration notes (gotchas)
- Smoke MPD is **0.24.0**. The **`playid` command does not exist** there → seek/play by **0-based
  position**: `play <pos>`. `Mpd::play_position(pos)` does this. Do not reintroduce `playid`.
- **Shuffle** is MPD's `random` mode (`random 0|1`), not a one-shot reorder. `PlayerStatus.random`
  carries the state.
- **Remove** is `delete <pos>`.
- `QueueEntry.id` is the MPD **SongId**; it does **not** equal `PlayerStatus.current_song.id` (the
  DB track id). Never compare the two.
- CUE albums share one `uri`; highlight by track `id`.

## Build & serve
- Backend: `cd backend && cargo build` → `target/debug/oxide-player`; `cargo test` runs unit tests
  (`library/scanner.rs`, `dsp/config.rs`).
- Frontend: `cd frontend && npm run build` runs `tsc` (type-check) then `vite build` to `dist/`.
- The backend serves `dist/` statically; UI-only changes need no backend restart. New API routes
  require a backend rebuild + restart.
