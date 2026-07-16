# AGENTS.md — Oxide

Compact conventions and hard rules for working in this repo. Loaded by the agent on every session.

## Stack

- **Backend:** Rust, Axum 0.8, Tokio, `mpd_client` 1.4 / `mpd_protocol` 1.0, `rusqlite` (bundled), `lofty` (tags), `serde_yaml`/`serde_json`, `clap`, `tokio-tungstenite`.
- **Frontend:** React 18 + TypeScript + Vite 5. **CSS Modules only** — no UI framework, no Tailwind.
- **Audio:** MPD (music player daemon) + CamillaDSP (DSP). The backend is a control/metadata layer in front of MPD.

## Repo structure

- This is a **Cargo workspace** (`Cargo.toml` at root, `members = ["backend"]`). The backend is the only workspace member; `cargo build` / `cargo test` work from the repo root **or** `backend/`.
- The built binary is **`target/debug/oxide-player` at the repo root** (not `backend/target/`).
- Frontend is a separate npm package under `frontend/`; the backend serves `frontend/dist/` as static files.

## Bug rule (hard)

- For **every reported bug** (direct or via GitHub issue), first write a test that **reproduces/confirms** the bug, then fix it. Add that test to the suite that runs **before pushing to `main`** (PR or direct push) so the bug can never silently regress.

## Build / run / test

```bash
# Backend (from repo root or backend/)
cargo build                                   # debug binary at target/debug/oxide-player
cargo test                                    # unit tests (scanner, dsp config, PWA static serving)
./target/debug/oxide-player -c config.json    # default listen 0.0.0.0:8000
#   flags: --mpd-host, --mpd-port, --listen

# Frontend
cd frontend && npm install
cd frontend && npm run dev            # Vite dev server (:5173)
cd frontend && npm run build          # tsc (type-check) + vite build -> frontend/dist/
cd frontend && npm test               # frontend tests
```

- **`cargo test` needs `frontend/dist/` to exist first.** A backend test asserts on the served PWA/static artifacts, and CI builds the frontend before running backend tests. Build the frontend (`npm run build`) before `cargo test`, or that test fails.
- The backend serves `dist/` from disk per request, so a UI-only change usually needs **no backend restart**; you **do** need a backend rebuild + restart when you add/change an API route.
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

See `ARCHITECTURE.md` for the full picture (module breakdown, full API reference, request flow) and `README.md` for install/usage.
