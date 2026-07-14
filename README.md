# Oxide

An audiophile music server: a Rust backend that controls [MPD](https://www.musicpd.org/)
(music player daemon) and [CamillaDSP](https://github.com/HEnquist/camilladsp) (DSP), with a
React + TypeScript web UI. Oxide is a thin control/metadata layer in front of MPD — it scans your
library into a SQLite database, serves cover art, and exposes playback, queue, shuffle, DSP and
device controls over a JSON API.

## Features
- Library browser (albums / artists / tracks) with search and cover art.
- Now-playing bar with play/pause/stop/next/prev, scrub, volume.
- **Queue panel** (open from the player bar) with click-to-jump, **shuffle** toggle, and per-track
  **remove**.
- Per-track 3-dot menu: play next, clear-and-play, add to playlist, file info.
- DSP profile editing and output device enable/disable.
- Kiosk mode (full-screen now-playing).

## Stack
- **Backend:** Rust, Axum 0.8, Tokio, `mpd_client` / `mpd_protocol`, `rusqlite` (bundled), `lofty`.
- **Frontend:** React 18 + TypeScript + Vite 5, plain **CSS Modules** (no Tailwind).
- **Audio:** MPD + CamillaDSP.

## Quick start
```bash
# 1. Backend
cd backend
cargo build
./target/debug/oxide-player -c config.json        # default listen 0.0.0.0:8000
#    flags: --mpd-host <host>  --mpd-port <port>  --listen <ip:port>

# 2. Frontend (served by the backend from frontend/dist/)
cd frontend
npm install
npm run build        # tsc + vite build -> frontend/dist/
# for live UI dev: npm run dev   (Vite dev server)
```
Open the backend URL (default `http://localhost:8000/`). After changing frontend code, rebuild with
`npm run build`; the backend serves `dist/` from disk, so a UI-only change usually needs no backend
restart. New API routes do require a backend rebuild + restart.

## Usage
- **Library:** browse albums/tracks, search, refresh the scan or re-scan cover art.
- **Queue:** tap the ☰ button in the player bar to open the queue; tap a track to jump to it,
  tap ✕ to remove it. The shuffle 🔀 button toggles MPD random mode (its state is reflected in the
  UI and in `PlayerStatus.random`).
- **DSP / Devices:** edit the CamillaDSP profile and toggle output devices from their views.

## API (selected)
| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/api/status` | Player state, volume, current track, `random` |
| GET | `/api/library` | Tracks (filter by `q`/`artist`/`album`) |
| GET | `/api/queue` | Queue entries + `current` playing position |
| POST | `/api/playback/play` | Play a URI |
| POST | `/api/playback/play-next` | Insert after current track |
| POST | `/api/playback/clear-play` | Clear queue and play track |
| POST | `/api/playback/shuffle` | `{ "on": bool }` — toggle MPD random mode |
| POST | `/api/playback/jump` | `{ "pos": u32 }` — jump to queue position |
| POST | `/api/playback/remove` | `{ "pos": u32 }` — remove from queue |
| POST | `/api/playback/{next,prev,stop,pause,seek,volume}` | Transport controls |
| GET/PUT | `/api/dsp` | CamillaDSP profile |
| GET/POST | `/api/devices[/{id}/enable\|disable]` | Output devices |
| POST | `/api/playlists/{name}/add` | Add track(s) to a playlist |

## License
See `LICENSE`.
