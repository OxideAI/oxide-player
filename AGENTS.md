# Repository Guidelines

Oxide — audiophile music server. Rust (Axum 0.8) backend that controls **MPD** + **CamillaDSP**, with a React 19 + TypeScript + Vite 8 frontend. The backend serves the built frontend from `frontend/dist/` and exposes a JSON REST API plus two WebSocket streams. Current backend version 0.11.1 (MIT, © 2026 OxideAI). Install/usage: `README.md`.

## Project Overview

- One Cargo workspace member (`backend/`, crate `oxide-player`, edition 2021); frontend is a separate npm package (`frontend/`).
- Backend is a control/metadata layer in front of MPD, not an audio engine (except the FFT visualizer, which captures via cpal).
- Linux is the target platform (installer, systemd, Bluetooth). Non-Linux builds work with a Bluetooth stub.
- Ship path: GitHub Actions + release-please → cross-compiled Linux tarballs (aarch64 + x86_64) attached to releases; release.yml smoke-verifies each package before upload (binary --version, dist hashed-asset grep, systemd-analyze, install.sh executable).

## Architecture & Data Flow

### Backend startup (`backend/src/main.rs`)
`tracing_subscriber` → `Cli::parse` → `Config::load` → `LibraryDb::open` + migrate → `DspManager::new` + `ensure_running` → `VisualizerAnalyzer::new` → `Mpd::connect` + `ensure_running` (warn-only) → `BluetoothManager::new` → `AppState::new` (spawns `dsp.seed`) + `spawn_status_poller` → `api::router(state)` → axum::serve with graceful shutdown (SIGINT/SIGTERM → `mpd.stop()`).

### State (`backend/src/state.rs`)
`AppState` is a `Clone` wrapper over `Arc<Inner>`:
- `config: RwLock<Config>` (+ `config_path: Option<PathBuf>`), `status: RwLock<PlayerStatus>`, `event_tx: broadcast::Sender<StatusEvent>` (cap 32)
- `db: LibraryDb` (rusqlite), `dsp: DspManager`, `mpd: Mpd`, `bluetooth: BluetoothManager`, `visualizer: VisualizerAnalyzer`, `radio: RadioManager`, `device_configs: ConfigFragmentManager`, `scan_lock: tokio::Mutex`, `config_restart_pending: AtomicBool`

### Data flow
- **Status**: `spawn_status_poller` (1s loop) refreshes `PlayerStatus` and broadcasts; queue mutations broadcast immediately (`broadcast_queue_now`). `/api/ws` sends a Status+Queue snapshot on connect, then live `StatusEvent`s (tagged serde enum `{"type":"status"}` / `{"type":"queue"}`), lag-tolerant. MPD error `No such song/file/directory` → `spawn_blocking` DB delete + `next` + `clear_error`; MPD unreachable → status reset to stopped.
- **Frontend → backend**: single `api` object of typed fetch wrappers (`frontend/src/api.ts`, 69 methods); `usePlayerStatus` (`frontend/src/ws.ts`) holds one WS to `/api/ws` with backoff reconnect (1s → 10s max) plus a one-shot REST fallback (on constructor throw or a 1.5s open-timeout). `useVisualizer` opens a second WS to `/api/visualizer` (~40fps `{bins, level}` frames; the server seeds a zeroed baseline frame on connect, while the hook itself holds `null` until the first real frame — `Visualizer.tsx` renders idle-pulsing bars for `null` frames while playing; under prefers-reduced-motion it paints a single static frame (bars at idle height) and halts the loop, while the truly empty frame appears only when stopped (`playing === false`)).
- **DB/scanner work** is offloaded via `spawn_blocking` (SQLite is `Arc<Mutex<rusqlite::Connection>>`).
- **CamillaDSP**: profiles rendered to YAML (`dsp/config.rs`), written to the config path, reloaded over WS `{"Reload":{"config":<path>}}` on 127.0.0.1:1234.
- **Radio**: `/api/radio` CRUD over user stations (`backend/src/radio.rs`), persisted to `<data_dir>/radio_stations.json` (atomic temp+rename, seeded with JFK Ibiza when missing or unparseable — a deleted seed stays deleted). Play = clear → `play_uri(url)` → `broadcast_queue_now`; the stream URL is the MPD URI itself, so `resolve_play_uri` is bypassed.
- **Frontend routing**: no router library. Custom `Route {tab, album}` state + `history.pushState`/`popstate` in `App.tsx` (tabs: library / playlists / radio / settings, matching the TABS array order); `/kiosk` is a separate full-page branch (pathname check), not part of `Route`.

## Key Directories

```
backend/src/          main, config, state, types, error, api/{mod,bluetooth},
                      radio, mpd/, library/{scanner,db}, dsp/{config,camilladsp,profile},
                      bluetooth/{linux,stub,types,input,mpd_integration},
                      visualizer/, devices/{config_fragment,include_injector}
backend/tests/        integration tests (pwa_static.rs)
frontend/src/         App, api, ws, types, util, animations, useVisualizer;
                      components/{LibraryView, SearchView, NowPlaying, QueueView,
                      TrackMenu, FileInfo, KioskView, Visualizer(+Controls), DspView,
                      DevicesView, PlaylistsView, ConfigView, SearchBar, OfflineBanner,
                      UpdateToast, InstallPrompt, ShortcutHelp/Toast, Reveal, RadioView,
                      playerHooks, useKeyboardShortcuts, shortcuts}
frontend/src/__tests__  vitest suites; *.test.tsx also colocated in components/
tests/                ui-smoke.sh (15-step E2E smoke via agent-browser)
contrib/              docker/Dockerfile (install.sh harness), systemd unit
docs/                 plans/ + brainstorms/ (dated design docs), designs/ (UI mockups),
                      screenshots/
data/                 runtime data_dir by default (./data); holds config.json +
                      radio_stations.json (seeded on first run); data/config.json is a dev
                      example but IS gitignored — absent on fresh clones
```

## Development Commands

```bash
cargo build                              # binary at repo-root target/debug/oxide-player
cargo test                               # needs frontend/dist to exist first (pwa_static.rs)
./target/debug/oxide-player -c config.json   # flags: --mpd-host, --mpd-port, --listen, --allow-root
cd frontend && npm install
cd frontend && npm run dev                # Vite :5173, proxies /api (ws) to 127.0.0.1:8000
cd frontend && npm run build              # tsc type-check + vite build -> dist/
cd frontend && npm test                   # vitest run; npm run test:watch for watch mode
./dev.sh                                  # OXIDE_MODE=prod (default): build frontend + serve :8000
                                          # OXIDE_MODE=dev: Vite; env: OXIDE_API_PORT, OXIDE_VITE_PORT, OXIDE_CONFIG
tests/ui-smoke.sh                         # needs running backend+frontend and a populated library
install.sh --update | --fix-perms         # installer modes (NOT backend CLI flags)
```

- Backend serves `dist/` from disk per request: UI-only changes need **no backend restart**; new/changed API routes need a backend rebuild + restart.
- No lint or format commands exist (see Runtime/Tooling).
- CI (`.github/workflows/ci.yml`): backend job builds frontend dist first (PWA MIME tests depend on it), then `cargo test`; frontend job runs `npm run build` + `npm test`. Linux build deps: `libasound2-dev libdbus-1-dev pkg-config` (cpal + bluer).

## Code Conventions & Common Patterns

### Hard rules
- **No Tailwind.** Styling is plain **CSS Modules** (`*.module.css`). Dark-only theme in `frontend/src/index.css`: CSS vars (`--bg` OLED black, `--accent` emerald, `--accent-2` indigo), mesh/grain background, `prefers-reduced-motion` kill switch.
- **Never commit** unless explicitly asked.
- Cove style: keep real logic, strip only boilerplate.

### Backend
- Errors: `AppError` (thiserror 2) → JSON `{"error": msg}`. Mapping: `Mpd→502`, `Library/Dsp→500`, `NotFound→404`, `BadRequest→400`, `Unprocessable→422`, `Bluetooth→400`, `BluetoothUnavailable→503`. Handlers use `anyhow` internally, `AppResult<T>` alias, `.map_err(|e| AppError::X(e.to_string()))`.
- Shared state via `AppState` (see above); config/status behind `RwLock`, scan behind `tokio::Mutex`; blocking work in `spawn_blocking`.
- JSON is serde `snake_case`; `PlaybackState` is `playing/paused/stopped`; WS events are tagged enums (`#[serde(tag = "type")]`).
- Config: JSON, precedence `--config path` → `<data_dir>/config.json` → defaults; atomic writes via temp file + rename (**no fsync**); CLI overrides applied when != default.
- Platform split only in `bluetooth/` (`#[cfg(target_os = "linux")]` bluer vs stub); everything else compiles everywhere.
- Naming: snake_case modules/files, body structs `*Body`, handler return `AppResult<Json<..>>`.

### Frontend
- No router/state/data libraries — custom path routing in `App.tsx`; `types.ts` mirrors backend JSON exactly.
- One `api` object (`api.ts`) with a `json<T>()` helper that throws `Error(body.error)`; `toPlayRef(t)` builds the `{uri, start, end, track_id}` envelope for play-next/clear-play/playlist-add.
- Live state is WS-push driven (`usePlayerStatus`); views fetch REST on mount; transport actions are optimistic via callbacks in `App.tsx`.
- `playerHooks.ts`: `useSmoothElapsed` (rAF interpolation between 1s server samples), `useDragValue` (120ms throttled commits, trailing commit).
- Shortcuts: `shortcuts.ts` `BINDINGS` table + global handler in `useKeyboardShortcuts.ts` (ignores modifiers and typing targets; note: `?` and `h` are both bound to help).
- Suites that exercise API calls mock the api module with `vi.mock('../api', ...)` (libraryView with `importOriginal` to keep real `toPlayRef`; RadioView with `importOriginal` overriding only the radio methods with spies; ConfigView full-mock); suites that render components without api calls (TrackMenu, pwa, persistence) need no mock.

### MPD gotchas (memory — easy to get wrong; smoke MPD is 0.24.0)
- **`playid` does not exist** → always seek/play by 0-based position (`Mpd::play_position`; `play_song_id` resolves id→pos). Never reintroduce `playid`.
- **Shuffle = MPD `random` mode toggle** (`random 0|1`), not a one-shot queue reorder. `POST /api/playback/shuffle {"on":bool}`; `PlayerStatus.random` reflects state.
- **Remove from queue = `delete <pos>`** → `POST /api/playback/remove {"pos":u32}`.
- `QueueEntry.id` is the MPD **SongId**; it does NOT match `PlayerStatus.current_song.id` (DB track id) — but `PlayerStatus.current_id` IS the MPD SongId. Never compare queue/status ids to DB ids.
- **CUE albums**: every track shares one `uri` → now-playing highlight matches by track `id`, not `uri`; `resolve_play_uri()` maps DB path → MPD-relative URI and `<stem>.cue/trackNNNN`; CUE end is best-effort (no `rangeid` before MPD 0.25).
- **Streams (internet radio)**: MPD fills the song Title of http(s) streams from icy-metadata → `MpdStatus.current_title`. Stream URIs never resolve from the library DB; `resolve_current_song` falls back to a `TrackRef` with `title = current_title` and `artist` = the matched station's name (`RadioManager::by_url`), so NowPlaying shows a meaningful row instead of a bare URL. Streams are **not seekable**: NowPlaying guards the scrubber + arrow keys when `duration <= 0` (seekcur raises an MPD error that surfaces as a status banner) and renders a pulsing `LIVE` badge instead of the time. (Reduced-motion gap: RadioView's liveDot kills its pulse; NowPlaying's `.live::before` does not.)
- Cover resolution special-cases CUE (album-named jpg + single-image-in-folder fallback).

### UI flow notes
- Track rows (LibraryView) are a CSS grid; the `@media (max-width: 640px)` block sets columns for ALL FOUR variants `.row / .rowActive / .rowPlaying / .rowPaused` — keep all four in the selector list. (`.rowActive` is currently dead CSS; SearchView has its own mobile block with 3 variants, no `.rowActive`.)
- The 3-dot `TrackMenu` (play-next / clear-play / add-to-playlist / file-info) appears on track rows and album headers.
- Bluetooth input (A2DP sink via bluealsa) is a TODO: Linux `input.rs` bails "not yet implemented (U4)" → 400.

## Important Files

- `backend/src/main.rs` — entry, component wiring order.
- `backend/src/state.rs` — `AppState`, `spawn_status_poller`, status refresh + `resolve_current_song` chain (by_elapsed → by_cue_address → by_active → by_uri_cue → by_suffix).
- `backend/src/api/mod.rs` — **70 routes total: 54 here** (status, library/scan/covers, playback, queue, ws, visualizer, devices, dsp, playlists, config, version, radio) **+ 16 in `api/bluetooth.rs`** (devices, scan, pair, connect, wake-connect, disconnect, forget, remove-output, rename, test-connect, input/*). `ServeDir(static_dir)` + SPA fallback.
- `backend/src/mpd/mod.rs` — `Mpd` wrapper (36 methods): lazy reconnect (5s timeout, double-checked via connect_lock), raw command passthrough, `ensure_running` autostarts a local mpd daemon (20×500ms retry).
- `backend/src/dsp/camilladsp.rs` + `config.rs` + `profile.rs` — profile model (BitPerfect/Resample modes, EQ bands), CamillaDSP v4.1.3 YAML render, WS reload.
- `backend/src/library/db.rs` — SQLite schema, FTS5 search (unicode61, prefix `tok*`), CUE addressing; `scanner.rs` — `.mpdignore`, incremental mtime scans, cover extraction/optimization.
- `backend/src/devices/config_fragment.rs` + `include_injector.rs` — per-output config fragments in `<data_dir>/mpd-outputs.d/` injected into mpd.conf; changes set `restart_pending` → UI restart banner.
- `backend/src/bluetooth/linux.rs` — bluer/BlueZ manager (discovery, pair, connect, wake-and-connect with retries); `stub.rs` mirrors the API elsewhere.
- `backend/src/visualizer/mod.rs` — cpal capture + rustfft, 72 log-spaced bins at 40Hz, broadcast cap 64; only active when `visualizer_fft`. Frontend matches bin count at runtime (`min(bins.length, MAX_BARS=256)`).
- `backend/src/radio.rs` — `RadioManager`/`RadioStation` (uuid v4 ids): CRUD with validation (trimmed non-empty name, http(s) URL, no duplicate URLs), synchronous atomic temp+rename persistence to `<data_dir>/radio_stations.json`, JFK Ibiza seed on missing/unparseable file; 7 unit tests. `MpdStatus.current_title` carries the stream's live icy-metadata title.
- `frontend/src/App.tsx` — routing + optimistic transport; `api.ts`, `ws.ts`, `types.ts` — wire layer; `playerHooks.ts`, `shortcuts.ts`; `components/RadioView.tsx` — add/list/play/delete stations, live-dot highlight on the playing stream (`s.url === nowPlayingUri` while playing), same CSS Module + `json<T>` error patterns.
- `data/config.json` — full dev config key list (mpd_autostart, camilladsp_autostart, visualizer_fft, default_dsp_profiles, ...); **gitignored**, so it exists only in this working tree, not fresh clones. (README's Config section instead points at `backend/data/config.json` — a runtime-generated file, also gitignored.)
- `install.sh` — idempotent Debian installer (apt deps, builds camilladsp v4.1.3, systemd units, samba/avahi/bluetooth); default `LISTEN=0.0.0.0:80`. `contrib/systemd/oxide-player.service` — User=oxide, Nice=-5; **known drift**: it lacks the `CAP_NET_BIND_SERVICE` that install.sh's fallback unit has, so the default :80 bind fails as User=oxide (install.sh prefers the contrib unit); `Documentation=` is a placeholder your-org URL.
- `ARCHITECTURE.md` — module tables + API reference, but **stale**: misses 34 of the 70 routes (visualizer/version/devices-configs/bt/radio — its table has 36 rows), the WS-primary status flow, modules bluetooth/, visualizer/, devices/, library/db.rs, radio/, lists the wrong App.tsx tabs (library/devices/dsp/playlists vs actual library/playlists/radio/settings), and its `cd backend && cargo build → target/debug/oxide-player` wording is ambiguous (workspace target lives at repo root); neither README nor ARCHITECTURE mention the radio feature at all; trust code, not the doc.

## Runtime/Tooling Preferences

- **Backend**: Rust edition 2021; tokio 1 (full), axum 0.8 (ws), tower-http 0.7 (fs/trace/cors), serde, mpd_client 1.4 + mpd_protocol 1.0, lofty 0.24, rusqlite 0.40 (bundled), serde_yaml 0.9, thiserror 2, tracing (+ env-filter), clap 4, tokio-tungstenite, image 0.25, rustfft, cpal, uuid 1 (v4, serde); Linux-only `bluer` 0.17. `Cargo.lock` committed.
- **Frontend**: npm (package-lock.json committed), **Node >= 26** (README's "Node 18+" is stale; install.sh's own check still says 18), React 19.2, Vite 8.1, TypeScript 7 (strict, noUnusedLocals/Parameters, ES2020 target, no path aliases), vitest 4, jsdom 30, @testing-library/react, vite-plugin-pwa (autoUpdate, `/api/cover/` CacheFirst 30d, other `/api/` NetworkOnly). Dev proxy `/api` → 127.0.0.1:8000 with `ws:true`.
- **No lint/format tooling anywhere**: no rustfmt.toml, no clippy config (`[lints]` absent), no eslint/prettier, no Makefile, no lint scripts, no CI lint steps. `tsc` via `npm run build` is the type gate.
- **External services**: MPD 0.24 (autostart via backend), CamillaDSP 4.1.3, BlueZ on Linux; default listen **127.0.0.1:8000** (installer defaults to :80; README is stale on both counts — install URLs say :8000 and its quick-start comment claims default listen 0.0.0.0:8000).
- **Versioning**: release-please, single rust package `backend`, tags `oxide-player-v*`. **Known bug**: the `extra-files: ["frontend/package.json"]` entry resolves relative to the package dir → `backend/frontend/package.json` (missing) → silently skipped; `frontend/package.json` is stuck at 0.9.0 while backend is 0.11.1 (fix would be `../frontend/package.json`). `frontend/CHANGELOG.md` is likewise stale (legacy `oxide-player-frontend-v*` component). Don't assume frontend/backend versions match.
- **Dependabot**: cargo `/backend` + npm `/frontend` weekly (limit 10 each), github-actions weekly.
- **Security**: `.envrc` (direnv) contains a plaintext `SUDO_PASSWORD` (plus SEARXNG_BASE_URL and a device comment) and is NOT in `.gitignore` — never commit it; treat repo pushes as needing a pre-push check for it.

## Testing & QA

- **Bug rule (hard)**: for every reported bug, first write a test that **reproduces/confirms** it, then fix; the test stays in the suite so the bug can never silently regress. Test must run before pushing to `main`.
- **Backend**: `cargo test` — unit tests colocated in src (verified snapshot: scanner ~22, library/db ~15, devices/config_fragment ~18, dsp/config ~10, config ~10, radio 7, plus dsp/profile, dsp/camilladsp tokio, include_injector, state tokio (incl. stream→station fallback), bluetooth/mpd_integration, api); integration `backend/tests/pwa_static.rs` (4 tests: manifest MIME, sw MIME, icon reachability, mime_guess mapping) **panics if `frontend/dist/sw.js` is missing** — build the frontend before `cargo test`.
- **Frontend**: vitest (jsdom 30, config inline in vite.config.ts) + testing-library; suites in `__tests__/` (libraryView — clear-and-play regression #32, pwa, trackMenu, persistence, appRouting — /radio parse + buildPath round-trip, album deep links, unknown-path fallback) plus colocated `components/ConfigView.test.tsx`, `TrackMenu.test.tsx` (portal regression #49) and `RadioView.test.tsx` (9 tests: render, client-side URL/name validation, add/play/delete flows, live-station state, empty state, error surfacing). `setup.ts` shims `globalThis.localStorage` for jsdom 30 opaque origins. Run: `cd frontend && npm test`.
- **E2E**: `tests/ui-smoke.sh` — **15 steps** via agent-browser (library render, search, open album, clear-and-play, playback POST regression guard, pause/resume, shuffle→status.random, queue, playlists, settings, kiosk, no console errors) against a running backend + populated library; asserts `/api/status` via curl + python.
- No coverage gates or mutation tooling in CI.
