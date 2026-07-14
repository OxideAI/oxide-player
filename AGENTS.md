# AGENTS.md — Oxide

Project conventions and hard rules for working in this repo. Loaded by the agent on every session.

## Golden rules
- **Write code like cove. All logic stay. Only boilerplate die.** Keep real behavior; strip only ceremonial boilerplate.
- **NEVER use Tailwind.** Styling is plain **CSS Modules** (`*.module.css`). New components get a co-located module.
- **NEVER commit changes** unless the user explicitly asks.
- User is **Stojce**.

## Stack
- **Backend:** Rust, Axum 0.8, Tokio, `mpd_client` 1.4 / `mpd_protocol` 1.0, `rusqlite` (bundled), `lofty` (tags), `serde_yaml`/`serde_json`, `clap`, `tokio-tungstenite`.
- **Frontend:** React 18 + TypeScript + Vite 5. **CSS Modules only** — no UI framework, no Tailwind.
- **Audio:** MPD (music player daemon) + CamillaDSP (DSP). The backend is a control/metadata layer in front of MPD.

## Build / run / test
```bash
# Backend
cd backend && cargo build            # debug binary at target/debug/oxide-player
cd backend && cargo test             # unit tests (scanner, dsp config)
./target/debug/oxide-player -c config.json   # default listen 0.0.0.0:8000
#   flags: --mpd-host, --mpd-port, --listen

# Frontend
cd frontend && npm install
cd frontend && npm run dev           # Vite dev server
cd frontend && npm run build         # tsc (type-check) + vite build -> frontend/dist/
```
- The backend **serves `frontend/dist/` as static files**. After a frontend rebuild you usually do **not** need to restart the backend (files are read from disk per request); you **do** need a backend rebuild + restart when you add/change an API route.
- `npm run build` runs `tsc` first, so it doubles as the type-check. Fix type errors there before declaring done.

## MPD gotchas (memory — easy to get wrong)
- Smoke/test MPD is **0.24.0**. The **`playid` command does not exist** there → always seek/play by **0-based position** (`play <pos>`). `Mpd::play_position(pos)` already does this. Do not reintroduce `playid`.
- **Shuffle = MPD `random` mode toggle** (`random 0|1`), not a one-shot queue reorder. Exposed as `POST /api/playback/shuffle { "on": bool }`; `PlayerStatus.random` reflects state. The queue button highlights when `random` is true.
- **Remove from queue = `delete <pos>`** → `POST /api/playback/remove { "pos": u32 }`.
- Queue positions are **0-based**. The `/api/queue` response returns `current` as the **playing position** (not an id) so it survives shuffles.
- `QueueEntry.id` is the **MPD SongId**. It does **NOT** match `PlayerStatus.current_song.id` (that is the **DB track id**). Never compare them directly.
- **CUE albums:** every track shares one `uri`. The now-playing highlight matches by track `id`, not `uri`. Cover resolution also special-cases CUE (album-named jpg + single-image-in-folder fallback).

## UI flow notes
- Queue panel + shuffle + remove live in `NowPlaying.tsx` / `QueueView.tsx`. The panel **refetches `/api/queue` every 1s while open** so the highlighted track follows next/prev.
- Track rows (LibraryView) are a CSS **grid**. The `max-width: 640px` rule must include `.rowPlaying` / `.rowPaused` or the 3-dot `TrackMenu` wraps to a new line. When editing mobile row CSS, keep all four row variants in the selector list.
- The 3-dot `TrackMenu` (play-next / clear-play / add-to-playlist / file-info) appears on each track row and on album headers.

## Layout
```
backend/src/   main, config, mpd, api, state, types, error,
               library/{scanner, …}, dsp/{…}
frontend/src/  App.tsx, api.ts, types.ts, util.ts,
               components/{LibraryView, NowPlaying, QueueView, TrackMenu,
                           KioskView, DspView, DevicesView, PlaylistsView}
```
See `ARCHITECTURE.md` for the full picture and `README.md` for usage.
