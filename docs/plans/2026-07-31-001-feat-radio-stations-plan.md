# Plan — Radio stations (stream playback + user-managed stations)

**Type:** feat
**Origin:** user request (2026-07-31): "add radio stations — for now just https://jfkibiza.es/, but the user should be able to add radio station manually"

## Problem frame

Oxide plays only library files (and saved playlists of those files). There is no way
to play an internet radio stream. We want:

1. A playable seeded station: **JFK Ibiza** (homepage `https://jfkibiza.es/`).
2. Users can add/remove stations manually from the UI (name + stream URL), persisted
   across restarts.

Playback rides the existing pipeline: MPD natively plays http(s) audio streams
(curl input plugin) — a stream is just another queue entry whose URI starts with
`http(s)://`. The backend already tolerates non-library URIs: `resolve_current_song`
falls back to a minimal `TrackRef { uri }` when the DB can't resolve the current URI,
so `current_song` keeps populating for streams and the NowPlaying UI doesn't go blank.

### Verified stream facts (probed 2026-07-31)

| Fact | Value |
|---|---|
| Homepage | `https://jfkibiza.es/` (WordPress; its "web player" is a TuneIn embed, station `s270143`) |
| Direct stream (seed) | `https://stream.aiir.com/7dsjltmny8cvv` |
| Behavior | 302 → `https://stream-{N}.aiir.com/7dsjltmny8cvv?zt=<JWT>` — **fresh signed token per request** (~90 s validity). The base URL is the bookmarkable one: each request re-signs, and libcurl/MPD follows the redirect. An established connection keeps playing past token expiry. |
| Stream type | `audio/aac` (ADTS, 48 kHz stereo, icy-metaint 16000), `icy-name: "JFK Ibiza"` |
| MP3 mirror | `https://nexstream.cloud/jfk` (radio-browser entry) — TLS handshake refused from this dev machine; reachability is network/geo dependent. Fallback candidate only. |
| Source of truth | radio-browser.info: "JFK Radio Ibiza" / `stream.aiir.com/7dsjltmny8cvv`, homepage `jfkibiza.es` |

## Scope boundary

- In scope: radio station store + CRUD API, "play station" (stream playback via MPD),
  stream-aware now-playing (icy title), Radio tab in the UI with add/delete forms,
  seeded JFK Ibiza.
- Out of scope: stream discovery/probing, stream cover art, per-station settings,
  TuneIn/radio-browser directory integration, scheduled/rotating stations, editing
  a station (delete + re-add covers it for v1).

## Key Technical Decisions

- **Streams play through MPD, queue-only.** Radio play = `clear` queue → `add <url>`
  → `play` → `set_active_track(None)` → `broadcast_queue_now`. The queue ends up with
  a single stream entry; next/prev/remove stay harmless. No new audio path, no
  `playid`, no range logic — all existing MPD gotchas are sidestepped because the
  stream is added by raw URL, bypassing `resolve_play_uri` (library-path mapping).
- **Store: `<data_dir>/radio_stations.json`**, modeled on `VizParams::load/save`
  (best-effort load, atomic temp+rename save). When the file is missing, the store
  seeds it with the JFK Ibiza entry — fresh installs get the station, users can
  delete it, and the seed never re-asserts itself once the file exists.
- **Model** (serde `snake_case`, mirrors frontend exactly):
  `RadioStation { id: String (uuid v4), name: String, url: String, homepage: Option<String> }`.
  `id` = `uuid` crate v4 (small, no transitive deps; not currently in the tree).
  URL is deduped on create.
- **API** (handlers in `api/mod.rs`; router adds 4 routes):
  - `GET /api/radio` → `Json<Vec<RadioStation>>`
  - `POST /api/radio` `{name, url, homepage?}` → created station; 400 on empty name,
    non-http(s) url, or duplicate url
  - `DELETE /api/radio/{id}` → 204; 404 unknown id
  - `POST /api/radio/{id}/play` → 204; 404 unknown id; MPD failure surfaces as 502
    (stream dead/unreachable) via the existing `AppError::Mpd` mapping
- **Now-playing for streams.** `MpdStatus` gains `current_title: Option<String>`
  (MPD `currentsong` `Title` tag — the live icy "now playing" track name). In
  `resolve_current_song`, when the URI is `http(s)://` and DB resolution fails:
  title = MPD `current_title`; `artist` = matched station's `name` (prefix/URL match
  against the radio store), else `None`; everything else stays `None`. No new
  `radio` flag in `PlayerStatus` — the frontend detects a stream by URI scheme.
- **Stream semantics in UI.** Streams report `duration: 0` / unreliable `elapsed`.
  `NowPlaying` already refuses scrubbing when `duration <= 0`; polish it to show
  "LIVE" instead of `0:00 / 0:00`. Queue view shows the stream entry as-is (MPD
  fills Title from icy metadata).
- **Error path is already safe.** A dead stream produces MPD errors like
  `problems opening http…` — these do NOT match the `"No such song/file/directory"`
  detector in `refresh_status`, so no DB deletes or auto-`next` get triggered. The
  UI just shows `status.error`.
- **Known behavior: the stream entry lingers in the queue.** Radio play clears the
  queue, but afterwards a library play (play/clear-play/playlist-play) leaves the
  stream URL entry in place — when the song ends, MPD can auto-advance back into
  the stream. Accepted for v1: entries are removable via the queue UI (`delete <pos>`
  works on stream entries). Stripping `http(s)://` entries inside the existing play
  endpoints is a possible follow-up, deliberately out of scope.

## Implementation Units

### U1 — Radio store: `backend/src/radio.rs`
`RadioStation` struct (Serialize/Deserialize/Clone), `RadioManager { stations: RwLock<Vec<RadioStation>> }`:
- `load(data_dir)` — read file; missing/unparseable → seed `[JFK Ibiza]` and save it.
- `list()`, `add(name, url, homepage)` (trim inputs; validate name non-empty, url scheme http/https; dedupe by exact URL match), `remove(id)`, `by_url(url)` (for status title lookup).
- Save: temp file + rename, same as `VizParams::save`.
- `AppState`: add `radio: RadioManager` field, constructed in `AppState::new` from `config.data_dir`.
**Files:** `backend/src/radio.rs`, `backend/src/state.rs`, `backend/src/main.rs` (module decl).

### U2 — MPD stream title: `backend/src/mpd/mod.rs`
- `MpdStatus` gains `current_title: Option<String>`.
- `Mpd::status()` already runs `client.command(CurrentSong)` in its `has_current`
  branch — read the `Title` tag off that `Song` (`song.get_tag(Tag::Title)`; icy
  metadata lands there for streams, harmless for files). When there is no current
  song, `current_title` stays `None`.
- Two `MpdStatus` literals in `state.rs` tests (lines ~457, ~516) need the new
  field.
**Files:** `backend/src/mpd/mod.rs`, `backend/src/state.rs` (test fixtures).

### U3 — Stream-aware now-playing: `backend/src/state.rs`
In `resolve_current_song`'s fallback branch: if `uri` starts with `http://`/`https://`, build the minimal `TrackRef` with `title = ms.current_title` and `artist = station name` (via `radio.by_url`), instead of the current all-`None` fallback. Non-http URIs keep today's behavior.
**Files:** `backend/src/state.rs`.

### U4 — API: `backend/src/api/mod.rs`
- Four handlers per the decision list (body structs `AddRadioBody`), registered in `router()` alongside the existing routes. `play` mirrors `clear_play` (which is the exact pattern: `clear → add → set_active_track → broadcast_queue_now`) but bypasses `resolve_play_uri` and uses `play_uri(url)` directly. Colocated unit tests per house style — but scoped to what runs without a live MPD: store round-trip, validation, dedupe, unknown-id 404s. The play path itself is covered by the manual smoke (existing api tests exercise pure helpers only).
**Files:** `backend/src/api/mod.rs`.

### U5 — Frontend wire layer: `frontend/src/types.ts`, `frontend/src/api.ts`
- `RadioStation` interface mirroring the backend model.
- `api.listRadio()`, `api.addRadio(name, url, homepage?)`, `api.deleteRadio(id)`, `api.playRadio(id)` — standard `json<T>()` wrappers.
**Files:** `frontend/src/types.ts`, `frontend/src/api.ts`.

### U6 — Radio tab: `frontend/src/App.tsx`
- `type Tab = 'library' | 'playlists' | 'settings' | 'radio'`, add `{ id: 'radio', label: 'Radio' }` to `TABS`, extend `parsePath`/`buildPath` (path `/radio`), render `RadioView` in the tab switch.
- **Routing test required** (ui-navigation skill bug-rule: any `Route` parsing/building change needs an RTL `*.test.tsx`): `parsePath('/radio')` → `{ tab: 'radio', album: null }`, `buildPath` round-trip, and album deep-links + fallback unchanged.
**Files:** `frontend/src/App.tsx`, `frontend/src/__tests__/routing.test.tsx` (or existing App test file).

### U7 — RadioView: `frontend/src/components/RadioView.tsx` + `RadioView.module.css`
- Fetch stations on mount (`api.listRadio`).
- Station rows: name (+ homepage link when present), Play / Stop-while-playing toggle, delete button. Playing indicator: `status.current_song?.uri === station.url && status.state === 'playing'` (usePlayerStatus).
- Add form: name + URL inputs, inline validation (empty name, non-http(s) URL), error display from API, adds and refreshes the list.
- Empty state ("No stations — add one below"), loading state. CSS Modules, dark-theme vars (`--accent`, `--bg` etc.), no Tailwind.
**Files:** `frontend/src/components/RadioView.tsx`, `frontend/src/components/RadioView.module.css`.

### U8 — LIVE polish: `frontend/src/components/NowPlaying.tsx`
When `duration <= 0`, render "LIVE" (accent dot + label) instead of `fmtTime(duration)`; progress bar stays inert (already guarded by `onScrubDown`'s `duration <= 0` early return at line ~51). **The keyboard seek path is NOT guarded**: the progress bar's `onKeyDown` (ArrowLeft/ArrowRight → `onSeek`) fires regardless of `duration`, and `seekcur` against a stream raises an MPD error that surfaces as a `status.error` banner. Gate the keydown handler on `duration > 0` (return early) in the same change.
**Files:** `frontend/src/components/NowPlaying.tsx` (+ module css if needed).

## Existing patterns to follow
- `VizParams::load/save` (atomic temp+rename, best-effort load) in `backend/src/visualizer/mod.rs`.
- `Mpd` command wrapper + `AppError` mapping (`Mpd→502`) in `backend/src/mpd/mod.rs`, `backend/src/error.rs`.
- Handler shape in `api/mod.rs`: `AppResult<Json<..>>`, body structs `*Body`, `AppError::BadRequest` for validation.
- Frontend: `api.ts` `json<T>()` helper, `usePlayerStatus`, CSS Modules only, `TABS` array in App.tsx.

## Test scenarios
- Backend unit tests (no live MPD): store (missing file → seeded JFK entry; roundtrip; dedupe rejects same URL; remove unknown id; validation rejects non-http(s) URL / empty name), API validation + 404s (create/list/delete/play-id-lookup), `MpdStatus` fixtures updated for `current_title`.
- Frontend vitest: `RadioView` (add-form validation, play/delete call `api` mocks, empty state), routing test for `/radio` parse/build + unchanged album deep-links (required by the ui-navigation skill), `NowPlaying` keydown guard (arrow keys don't fire `onSeek` when `duration <= 0`).
- Manual smoke: play JFK from Radio tab → audio via MPD, `status.current_song` shows stream URL, title = icy "now playing" once the stream pushes it; stop returns to library; arrow keys on the inert progress bar produce no `status.error`; add a bogus URL → clean error, no MPD corruption; delete station; restart backend → stations persist; JFK survives restart.
- MPD compatibility (verify on the real smoke MPD 0.24): the 302→signed-URL chain must be followed by MPD's curl input (libcurl follows redirects by default) and the ADTS-AAC stream must decode (needs MPD's ffmpeg plugin — Debian `mpd` ships it). If either fails, fall back to the MP3 mirror `https://nexstream.cloud/jfk` (needs reachability check from the target network) or document the MPD build requirement.

## Dependencies / sequencing
- U1 → U4 (API needs the store); U2 → U3 (status title); U5 → U6 → U7 (wire → tab → view); U8 independent.
- New crate: `uuid` (features `v4`, `serde`). Nothing else — no npm deps.
- Backend + frontend ship together (single release-please package).

## Risks
- **Signed CDN URL.** The aiir base URL re-signs per request and libcurl follows the 302, so this works with MPD — but it is a redirect-dependent stream. Verify in the MPD smoke before finalizing the seed URL; the MP3 mirror is the fallback.
- **AAC decode.** ADTS-AAC needs MPD's ffmpeg plugin. Debian's mpd includes it; verify on target.
- **Stream URL rot.** Radio CDNs change endpoints; user-managed stations + easy delete mitigate. JFK's URL is tracked in radio-browser and TuneIn if it ever needs refresh.
- **Geo/network reachability.** `nexstream.cloud` refused TLS from this dev machine; `stream.aiir.com` worked. The seed uses the verified-working URL.
