# Plan — Second Library View: Browse by Physical Folders + Bulk Play

**Type:** feat
**Origin:** user request (2026-08-20): "add second view to the library - by the physical folders. Also folders can be added to 'play now'/'clear and play' commands. Plan it first before implementation"
**Status:** implementation-ready
**Date:** 2026-08-20

## Problem frame

Library is the default tab in Oxide. Today `LibraryView` presents a single flat view: every track is bucketed by its **containing directory** (`folderKey(t.uri)` — `uri.slice(0, lastSlash)`) then rendered as a tile grid labeled with `t.album || dirname`. This happens to follow the filesystem, but it is not a true folder browser:

- No hierarchy. `Artist / Album / Disc 1` and `Artist / Album / Disc 2` are two unrelated tiles; there is no way to navigate up to `Artist / Album` or `Artist`.
- No empty-folder or intermediate-folder concept. Only leaf dirs that contain tracks appear.
- Tile name uses album metadata (`t.album`), not the folder basename, so it reads as an "album" grid, not a "folders" grid.
- Bulk play exists only at album scope (all tracks sharing one `folderKey`). The user cannot play an entire artist folder, a disc parent, or any arbitrary directory.

What we want: a **second view** inside Library that browses the actual physical folder tree, plus the ability to **play a folder** (Clear & Play / Play Next / Add to Playlist) covering every track recursively under it. The existing album-tile view stays as-is (default).

## Scope boundary

**In scope**

- A toggle inside the Library tab between **Albums** (today's view, unchanged as default) and **Folders** (new view).
- Folders view: hierarchical directory browser derived from the physical library layout (`path`/`uri` + `source`). Breadcrumb + up-navigation, list of immediate subfolders, list of tracks directly inside the current folder. Empty view / loading / error parity with Albums.
- Folder bulk actions: **Clear & Play** (replace queue, start at first track), **Play Next** (insert after current song, preserving order), **Add to Playlist** — all acting on the track set of a folder (recursive, sorted). A folder row and the folder header both expose the same 3-dot menu.
- Deep links + history: folder paths are URL-addressable and survive back/forward and refresh.
- Persistence: last chosen view (albums vs folders) remembered across reloads.
- Cover handling per folder (reuse existing album-keyed cover pipeline).

**Out of scope**

- Filesystem mutations (rename, move, delete, create folder) from the UI.
- A new backend folder-tree endpoint for v1 (see KTD-1). Deferred unless load testing shows frontend tree build is a bottleneck.
- Inotify / filesystem watcher and incremental folder diff streaming.
- Per-folder metadata (folder.jpg vs cover art, folder description, custom sort) beyond what `Track.cover_key` already gives us.
- Search index changes. Existing global Search stays tag/FTS-based; folder-view search is a local filter like Albums.
- Queue or playlist changes on the backend — bulk play rides the existing `POST /api/playback/play-next` and `POST /api/playback/clear-play` envelope which already accepts `N` tracks.

## Key Technical Decisions

### KTD-1 — No new backend endpoint for v1; tree is built on the frontend from `librarySnapshot`

The frontend already holds the full `Track[]` (via `GET /api/library` with ETag, cached to IndexedDB + localStorage — `frontend/src/libraryCache.ts`). A folder tree is a pure projection of that array: group by directory of `path` (absolute) / `uri` (relative), scoped by `source`. Building it is `O(N)` string splits; at 50k tracks this is < 20 ms and runs once per library load inside a `useMemo`.

Alternatives: new `GET /api/library/folders` returning `FolderNode[]` and `GET /api/library/folder?path=…` for track listing. More precise for huge libraries and would let the backend enumerate empty/ignored dirs, but it duplicates projection logic, adds a cache-invalidation surface, and the current app has no library too large for the pre-existing full fetch (album view already pays that cost). For v1 we keep the read path single — one GET, two views — and reserve the folder endpoint as a follow-up if profiling shows pressure (see Risk R1).

Deferred contract if later needed: `FolderNode { path: string; name: string; trackCount: number; coverKey: string | null; children: string[] }` plus `GET /api/library/folders?prefix=`.

### KTD-2 — Folder identity is absolute directory, display is relative

The `tracks` row carries both `uri` (relative to its library source, e.g. `Artist/Album/01.flac`) and `path` (absolute, e.g. `/mnt/music1/Artist/Album/01.flac`) and `source` (the configured `library_dirs` entry that produced it). Two different sources can have the same relative `Artist/Album` layout — keying on `uri` dirname alone would merge unrelated physical folders.

Identity rule: the canonical folder key is the **absolute directory** `path.slice(0, path.lastIndexOf("/"))`. For the tree parent/child relations we split that absolute path. For the URL we encode the absolute path; for the human label we render the last segment (`basename`) and the breadcrumb renders each segment. When `source` is available we can show a small source badge/chip on top-level folders to disambiguate mount roots; basename collision below top level is rare and needs no extra disambiguation because full path is in the breadcrumb/title.

Fallback: when `path` is empty or unexpectedly relative, fall back to `source + "/" + folderKey(uri)` as key so every row still sorts into a folder.

Encoding: absolute paths contain `/`. The route segment therefore stores one `encodeURIComponent(absoluteDir)` token (slashes become `%2F`, so the custom `Route` parser keeps a single segment). Avoids introducing a catch-all router.

### KTD-3 — Reuse `toPlayRef` + existing bulk-play endpoints for folder play

Folder play is "collect all tracks recursively under dir X, sort by `trackOrder` (today's `cue_index ?? track` then `displayTitle`), map each `t -> toPlayRef(t)`, call `api.clearAndPlay(refs)` / `api.playNext(refs)` / `api.addToPlaylist(name, refs)`". No new backend shape. The mapping already carries `start`/`end` for CUE splits via `toPlayRef`, and `resolve_play_uri` on the backend already handles CUE vs non-CUE and absolute-path addressing. Queue insertion order already reverses-iterates to preserve order (`play_next` / `clear_play` handlers), so the frontend must pass refs in display order and let the backend handle reversal.

Limit: very large folders (thousands of tracks) will POST a large JSON body. Accept for v1; MPD queues accept thousands of entries and the existing album path already does the same envelope. If a deploy hits payload limits, the follow-up is a lighter `POST /api/library/folder/play { folderPath, mode }` that fans out on the backend — intentionally not built now.

### KTD-4 — Routing: extend `Route` with library view + folder path, keep existing album links stable

Today `Route = { tab, album }`, `parsePath` maps `"/library" -> { tab:"library", album:null }` and `"/library/<encoded>" -> { tab:"library", album: decoded }`, `buildPath` mirrors it. Changing that encoding would break bookmarks and the ui-smoke flow (which covers clear-and-play on the album tile).

Plan: `type LibraryViewMode = "albums" | "folders"` and

```ts
interface Route {
  tab: Tab;
  album: string | null;          // albums mode selection (existing)
  libraryView: LibraryViewMode;  // default "albums"
  folderPath: string | null;     // absolute dir when in folders mode
}
```

URL scheme (no router library — same `history.pushState` / `popstate` in `App.tsx`):

- `/library` and `/library/<album>` → albums mode (backward compatible, `libraryView` defaults to `"albums"` when not specified).
- `/library/folders` → folders mode, root.
- `/library/folders/<encodedAbsoluteDir>` → folders mode at that dir (`encodeURIComponent(absoluteDir)`). Single segment after `folders`, so split('/') parsing stays trivial.
- Unknown path → fallback to `{ tab:"library", libraryView:"albums", album:null, folderPath:null }` (current `parsePath` fallback).

`buildPath` emits the same patterns. View toggle (Albums ↔ Folders) does `pushState` to the corresponding URL; inside Folders, entering a subfolder / breadcrumb / back button pushes `/library/folders/<encoded>`. The persistence layer (localStorage key `oxide:libraryViewMode`) restores the preferred view on `/library` root visits but explicit URLs win over the stored preference.

Alternative rejected: query param `?view=folders&path=…` — less bookmark-friendly and would differ from the existing path-param convention.

### KTD-5 — One new view component, one shared folder utility, minimal surgery to existing LibraryView

Add `frontend/src/components/FolderBrowseView.tsx` (and `FolderBrowseView.module.css`) for the folders view. Extract pure folder helpers into `frontend/src/folderTree.ts` (or `frontend/src/util.ts` if tiny) so both the view and tests can import them without rendering. Keep `LibraryView.tsx` focused on the albums grid + album detail, but share the `tracks` load + cache + search shell between the two views via a thin wrapper or via `App.tsx` lifting `tracks` state up. Preferred shape for v1 (least churn, least risk):

- Lift the library fetch (`api.librarySnapshot` + `libraryCache`) into a small hook `frontend/src/useLibraryTracks.ts` (state: `tracks, loading, refreshing, error, refresh`). Both views consume it. `LibraryView` and `FolderBrowseView` become presentation + track-order/filtering; the hook owns the network/cache concern (today that concern lives duplicated between LibraryView and SearchView).
- `App.tsx` holds the `libraryView` / `folderPath` route state, renders either `<LibraryView>` or `<FolderBrowseView>` inside the same Library tab, and passes `nowPlayingId/playing` etc. through unchanged.
- The toolbar's view switch is a two-segment control (Albums | Folders) sitting next to the existing search/refresh pills — same CSS Module vars, no new dependency.

This keeps the diff reviewable: new file for folders, one extracted hook, narrow edits to `App.tsx` + routing helpers, no behavioral change to albums unless folders is active.

### KTD-6 — Track ordering and recursive expansion

Folder bulk actions act on **all descendant tracks recursively**, not just direct children — a user entering `Artist` and hitting Clear & Play expects the whole artist, not an empty folder. Order: depth-first by folder path lexicographically, and within each folder by existing `trackOrder` (`cue_index ?? track` then `displayTitle`). This matches the album view's within-album order and keeps CUE albums (whose parts share one `uri`) in index order.

De-duplication: group by `track.id` when flattening (a CUE album expands to many rows with the same `uri` but distinct `cue_index`/`track.id` — keep them all). Direct-child listing in the view itself is non-recursive: the track list shows only tracks whose immediate parent is `currentDir`; subfolder tracks are visible only after navigating in, but the play-all action recurses.

### KTD-7 — Folder covers without a new pipeline

Reuse today's cover model: `Track.has_cover` + `cover_key` (album-keyed, one file per album). For a folder, pick the first descendant track (DFS order) with `has_cover && cover_key` and use `api.coverUrl(coverKey)`. No new scan step, no folder.jpg discovery. If no descendant has cover, show the `♪` placeholder (same as tile fallback). If a future sweep wants true folder art, the place to add it is `scanner::find_local_cover(dir)` returning a `folder_cover_key` per directory and persisting it on a new `folders` table or on `Track`'s `cover_key` fallbacks — deferred.

## Implementation Units

### U1 — Folder helpers + hook extraction

**Goal:** pure, testable folder math and a shared library-load hook, no UI.

**Files:**
- `frontend/src/util.ts` (extend — add `folderDir`, `folderBasename`, `folderAncestors`, `encodeFolderPath` / `decodeFolderPath` helpers; or new `frontend/src/folderTree.ts` if the file would grow > ~120 lines — prefer a new file to keep `util.ts` small)
- `frontend/src/useLibraryTracks.ts` (new — `useLibraryTracks(refreshToken)` wrapping `readLibraryCache` / `api.librarySnapshot` / `writeLibraryCache`, same semantics as today's `LibraryView.load`)
- `frontend/src/libraryCache.ts` (no change, referenced for cache contract)

**Behavior:**
- Helpers: `absoluteDirOf(t: Track): string` (from `t.path` or fallback composite), `buildFolderTree(tracks: Track[]) -> { nodes: Map<absDir, FolderNode>, children: Map<parent, string[]>, directTracks: Map<absDir, Track[]>, rootAbsDirs: string[] }`, `descendantTracks(dir, tree) -> Track[]` (recursive, sorted), `sortedSubfolders(dir, tree) -> string[]`, `ancestors(dir) -> string[]` for breadcrumbs.
- Hook: exposes `{ tracks, loading, refreshing, error, refresh, setError }`, handles cache-then-network + etag, used by both views.

**Tests:** `frontend/src/__tests__/folderTree.test.ts` (new) — building a tree from a small synthetic `Track[]` with two sources containing a colliding relative path, single-file root, nested dirs, CUE tracks, sort stability, descendant flatten, encode/decode round-trip.

### U2 — Folders view component

**Goal:** hierarchical browsing at `/library/folders` with breadcrumb, subfolders, and track list, plus folder header actions.

**Files:**
- `frontend/src/components/FolderBrowseView.tsx` (new)
- `frontend/src/components/FolderBrowseView.module.css` (new, CSS Modules only)
- `frontend/src/types.ts` (no change — reuses `Track`)

**Behavior:**
- Props: `{ tracks, loading, refreshing, error, nowPlayingId, isPlaying, folderPath, onFolderChange, query, onQueryChange }` — or consumed via hook + lifted state; match whichever U1 extracts.
- Renders: when `tracks.length===0` reuses Library empty state; else breadcrumb row (`Root / … / basename`, each segment clickable + Back), subfolders section (list/grid of rows showing name, track count, cover thumbnail), tracks-in-this-folder section (same `.row` grid as album detail — reuse `.list`/`.row*` classes or duplicate with `FolderBrowseView.module.css`). Each folder row and the header expose `TrackMenu` wired to the recursive track set for that dir (see U4). Track rows expose `TrackMenu` per track + click-to-play (`api.clearAndPlay([toPlayRef(t)])`).
- Search: same shell as Albums; when `query` is non-empty, filter folders and tracks by `basename/title/artist` substring (local filter, no backend call), plus optionally flatten search to show matching descendant tracks under highlighted parents.

**Tests:** `frontend/src/__tests__/folderBrowseView.test.tsx` (new, vitest + @testing-library/react) — render with synthetic tree: root lists subfolders, entering a subfolder updates breadcrumb, track rows render, play/clear wiring mocked via `vi.mock('../api')`, empty state.

### U3 — Routing + view toggle integration

**Goal:** second view is now reachable, bookmarkable, and persistent, without regressing album routes.

**Files:**
- `frontend/src/App.tsx` (modify `Route`, `parsePath`, `buildPath`, tab render, localStorage preference)
- `frontend/src/__tests__/appRouting.test.tsx` (extend — required by ui-navigation skill bug-rule whenever `Route` parsing/building changes)

**Behavior:**
- Extend `Route` per KTD-4, implement `parsePath`/`buildPath` for `/library`, `/library/<album>`, `/library/folders`, `/library/folders/<encoded>`.
- Render path inside the Library tab: when `libraryView==='folders'` show `FolderBrowseView`, else `LibraryView`. The top toolbar gains an Albums | Folders segmented control that `pushState`s the counterpart URL. On mount at `/library` (no subpath), select `localStorage.getItem('oxide:libraryViewMode')` as initial view; otherwise URL wins.
- `LibraryView` keeps `onAlbumChange` but now clears `folderPath` and vice-versa via App's navigation handler so the two detail states don't leak across views.

**Tests (mandatory):**
- `parsePath('/library')` -> albums root, `parsePath('/library/folders')` -> folders root, `parsePath('/library/folders/<enc>')` round-trips via `buildPath`, album deep links `parsePath('/library/<album>')` still parse, unknown path fallback, encode of paths containing spaces/`#`/`%`.

### U4 — Folder bulk play (Clear & Play / Play Next / Add to Playlist)

**Goal:** any folder can be played as a whole, reusing the existing MPD queue endpoints.

**Files:**
- `frontend/src/components/FolderBrowseView.tsx` (wire folder menu)
- `frontend/src/components/TrackMenu.tsx` (no change — already accepts `tracks: Track[]`; confirm it handles large arrays and the portal/regression #49 path)
- `frontend/src/api.ts` (no change — already exports `playNext`, `clearAndPlay`, `addToPlaylist`)
- `backend/src/api/mod.rs` (no change for v1 — `into_tracks` + `play_next`/`clear_play` already loop tracks in correct order with CUE handling via `resolve_play_uri`)

**Behavior:**
- Folder header's `TrackMenu` and each folder row's inline menu call `api.clearAndPlay(descendantTracks.map(toPlayRef))`, `api.playNext(...)`, `api.addToPlaylist(name, ...)`. Toasts surface "Added 42 tracks to queue" via the same `notify` bridge LibraryView uses.
- Guard: if `descendantTracks.length===0` (empty intermediate dir pruned from DB) disable the menu items or show "No tracks in this folder".
- Ordering per KTD-6; CUE handling per `toPlayRef` (passes `track.start`/`track.end`).

**Tests:**
- Unit: `frontend/src/__tests__/folderPlay.test.ts` or colocated in `folderBrowseView.test.tsx` — given a tree with nested folders + CUE tracks, `descendantTracks(root)` returns expected `toPlayRef` sequence, `api.clearAndPlay` receives them in that order (mock assertion), and empty-folder guard disables the action.
- Manual smoke: create a nested music tree (`Artist/Album/Disc 1`, `Disc 2`, single-file root), scan, open Folders at root, Clear & Play on `Artist` -> queue length equals all descendant tracks and playback starts at first; Play Next on a subfolder inserts directly after current song; Add to Playlist on a folder populates the saved playlist; CUE album inside a folder plays with correct `start`/`end` via existing backend CUE logic.

### U5 — Polish, empty states, and perf

**Goal:** no uncanny loading gap and no perf cliff on large libraries.

**Files:**
- `frontend/src/components/FolderBrowseView.module.css` (hover, reduced-motion, mobile `@media (max-width: 640px)` keeping all `.row` variants in the selector list per AGENTS.md warning)
- `frontend/src/components/LibraryView.tsx` (optional micro-copy: "Albums · 120 / Folders · 86 dirs" when both views are available)
- `frontend/src/useLibraryTracks.ts` (memoization: `useMemo` for the tree so rebuild runs only when `tracks` identity changes)

**Behavior:**
- Loading skeleton: reuse Albums spinner while `loading && tracks.length===0`; once tracks are cached, folder tree appears instantly and refresh runs in background (`refreshing` banner, same as Albums).
- Mobile: folder rows collapse `tArtist`/`tAlbum` columns like the existing `.row` mobile block; keep `.row / .rowPlaying / .rowPaused` in the selector list.
- A11y: folder rows are buttons with `aria-label` including trackCount; breadcrumb nav has `aria-current`.

**Tests:** snapshot-free assertions for reduced-motion and mobile class presence if the reviewer wants them; otherwise covered by existing css hygiene.

## Existing patterns to follow

- **Backend scan/DB:** `library/scanner.rs:scan` ingests into `tracks(path, uri, source, ...)` preserving absolute `path`; `library/db.rs:search` / `LibrarySnapshot` serve the full list; `source` disambiguation pattern `albums_with_sources()` shows how to handle parent/child sources (issue #46). Folder logic should mirror that source handling, not add a new table for v1.
- **API envelope:** `TrackRef { uri, start, end, track_id }` + `into_tracks` + `resolve_play_uri` → `mpd.play_next` / `play_uri_range`. Folder bulk play must call `toPlayRef(t)` per track — do not invent a `{ folderPath }` body for v1 (see KTD-3 deferred endpoint).
- **Frontend wire:** `api.ts:json<T>()` throws `Error(body.error)`; `api.librarySnapshot(etag)` with ETag/304; `libraryCache.ts` (IndexedDB `oxide-player/snapshots/library-v1`, fallback localStorage `oxide:library:v1`).
- **Routing:** custom `Route` + `history.pushState`/`popstate` in `App.tsx` (no router lib), `TABS` array order drives tabs; `/kiosk` is a pathname branch, not a `Route` tab — keep that invariant. Any `Route` change **must** add `frontend/src/__tests__/appRouting.test.tsx` (ui-navigation skill bug-rule, also applied for radio's `/radio`).
- **Styling:** CSS Modules only (`*.module.css`), dark theme vars from `frontend/src/index.css` (`--bg`, `--accent`, `--text-faint`, mesh/grain), `prefers-reduced-motion` kill switch. Never Tailwind.
- **Track ordering:** `trackOrder(a,b) = (cue_index ?? track) -> displayTitle` in `LibraryView.tsx` — reuse verbatim for folder track sort.
- **Menus:** `TrackMenu.tsx` already accepts `tracks: Track[]` and offers `play-next` / `clear-play` / `add-to-playlist` / `file-info` via a portal; reuse it for folders (pass `descendantTracks`) and single tracks.
- **Types:** `frontend/src/types.ts:Track` mirrors `backend/src/types.rs:Track` field-for-field; `TrackRef` for playback is separate and smaller.

## Test scenarios

### Unit (no live MPD — mock `api`)

- **Folder helpers (`folderTree.test.ts`):**
  - Empty library -> `{ nodes: {}, roots: [] }`.
  - Single track at `/a/b/c.flac` produces ancestors `/a`, `/a/b` in the map, `directTracks["/a/b"] === [track]`, `sortedSubfolders("/")` variant not needed — roots are top-level abs dirs.
  - Two sources with same relative `Rock/Album/song.flac` keyed as two absolute dirs (`/mnt/music1/Rock/Album` vs `/mnt/music2/Rock/Album`) remain distinct; `buildTree` puts each under its own root and global search returns both.
  - Nested `Artist/Album/Disc 1` + `Disc 2` — `descendantTracks("Artist/Album")` (absolute) returns tracks from both discs in lexicographic folder order then `trackOrder` inside each folder.
  - CUE siblings: three rows share `uri="Album/audio.flac"` with `cue_index 1..3` — `trackOrder` keeps 1,2,3 regardless of title sort fallthrough.
  - Encoding round-trip: absolute path with spaces and `#` -> `encodeURIComponent` -> `parsePath` -> `folderPath` equals original.
- **Hook (`useLibraryTracks`):** cache-hit path hydrates `tracks` before network; 304 does not rewrite cache; new etag does.
- **Folder play (mock `api`):** given a sample tree, `clearAndPlay` receives `tracks.map(toPlayRef)` in descendant DFS order; `playNext` called after `clearAndPlay` respects reverse-iteration contract on the backend (assert mock call order only, not MPD behavior).

### Component (vitest + jsdom + testing-library)

- `FolderBrowseView` renders root: breadcrumb shows "Library", subfolders list includes children of roots, direct-root tracks list shows root-level files (if any).
- Clicking a folder row navigates (mock `onFolderChange` called with absolute dir); Back/breadcrumb navigates up to parent or root.
- Track row click calls `api.clearAndPlay` with that single track; its `TrackMenu` "Play next" calls `api.playNext`.
- Folder header menu "Clear & Play" calls `api.clearAndPlay` with descendant set; toast appears; empty folder shows disabled menu + "No tracks" hint.
- Filter: typing in search shell filters visible subfolders and tracks but does not mutate the underlying tree.
- No regression: `LibraryView` still renders albums grid when `libraryView==='albums'`; toggling Albums <-> Folders swaps the child without remounting the page and updates URL.

### Routing (required)

- `parsePath('/library')` -> `{ tab:'library', libraryView:'albums', album:null, folderPath:null }`.
- `parsePath('/library/Some%20Album')` -> albums, `album==='Some Album'`.
- `parsePath('/library/folders')` -> `{ tab:'library', libraryView:'folders', folderPath:null }`.
- `parsePath('/library/folders/' + encodeURIComponent('/mnt/music1/Artist/Album'))` round-trips via `buildPath`.
- `parsePath('/unknown/path')` falls back to albums root; `buildPath` for a folders deep link survives refresh.

### Integration / manual smoke (needs running backend + populated library)

- Prepare a tree: `Single/song.flac`, `Artist/Album/Disc 1/{01,02}.flac`, `Disc 2/{01,02}.flac`, one CUE album `Cue/CueAlbum/{audio.flac, audio.cue}`.
- Scan, verify Albums still shows tiles; switch to Folders, verify root shows `Artist`, `Cue`, `Single` + correct counts; drill into `Artist -> Album`, verify breadcrumb `Library / Artist / Album`, back button returns to `Artist`.
- Play single track from a folder's track list -> `NowPlaying` highlights row (`isPlaying` + `.rowPlaying`) and elapsed advances.
- Folder Clear & Play on `Artist` -> queue length equals all descendant tracks, first track autoplays; next/prev traverse inside the queued folder set.
- Play Next on subfolder `Disc 2` while something is playing -> entries appear immediately after current `pos` (check `/api/queue` or WS `Queue` event) in display order.
- Add to Playlist on a folder -> saved playlist `m3u` (checked via `GET /api/playlists/:name` or Playlists tab) contains those `mpd_uri` entries in order; CUE entries resolve to `.cue/trackNNNN` URIs via `resolve_play_uri`.
- Search within folders (`"disc"` query) filters to matching dirs/tracks without extra backend calls.
- Refresh after navigation: `/library/folders/%2Fmnt%2F...%2FAlbum` still renders the same drill-down.

## Dependencies / sequencing

- U1 (helpers + hook) -> U2 (view) and U3 (routing) can proceed in parallel once helper signatures are sketched.
- U4 (bulk play) depends on U1's `descendantTracks` + `toPlayRef` ordering contract and on U2's menu placement.
- U5 (polish) last.
- No backend dependency for v1; if the backend folder endpoint is later revived, it inserts before U2 as U0b and becomes the view's data source with a feature flag.
- Backend + frontend ship together (single release-please package). No new Cargo crates, no npm deps.

## Risks

- **R1 — Large libraries and in-memory tree cost.** A library of 100k tracks produces ~10k dir nodes. `buildFolderTree` iterating once and a DFS `descendantTracks` per folder action is cheap, but a folder Clear & Play payload serializes many `PlayRef`s; very large JSON bodies can hit proxy/body limits and MPD `add` latency. Mitigation: memoize tree + descendant lists with `useMemo`, send in display order and trust the backend's existing batch `add` path; follow-up if seen in the wild is a backend-side `POST /api/library/folder/play { folderPath, mode }` that fans out in `api/mod.rs` using `LibraryDb` enumeration instead of transport.
- **R2 — Source collision.** Two library sources with identical internal structure must not merge. Mitigation: KTD-2 keys on absolute `path`; add a source chip at top level; unit test covers collision (_trust the test to fail if someone later keys on `folderKey(uri)` alone_).
- **R3 — URL encoding of absolute paths.** Absolute dirs contain slashes, spaces, `#`, `?`, `%`. A naïve `split('/')` router would mis-parse. Mitigation: store one encoded segment (`encodeURIComponent(absDir)`) after `/library/folders/`; `decodeURIComponent` once. Test round-trips with ` `, `#`, `%2F`, and unicode.
- **R4 — Pruned / ignored directories.** Empty dirs, `.mpdignore`-ed subtrees, and unreadable dirs never enter `tracks` and thus never appear — correct behavior (matching current Albums semantics). Do not surface filesystem entries that have no tracks, or the browse would diverge from the library the player can actually see. Document this in the view's empty state ("Only directories containing scanned tracks appear").
- **R5 — Cover reuse.** Folder covers fall back to the first descendant's `cover_key`. A folder whose tracks are split across many albums picks one album's art arbitrarily — acceptable for v1 and matches the per-album key dedup (issue #31). True folder art via `folder.jpg` lookup is the deferred step.
- **R6 — Deep state leaking between views.** Album selection (`album`) and folder path (`folderPath`) are mutually exclusive; toggling views must reset the other's selection so back/forward and shared state don't show an album detail while `libraryView==='folders'`. Guard in `App.tsx` navigation handler and test with the Albums -> Folders -> Back sequence.
- **R7 — Routing regression.** The custom `Route` is global. Any `parsePath`/`buildPath` typo breaks deep links, kiosk (`/kiosk` branch), and radio/playlists. Mitigation: extend `appRouting.test.tsx` and run `npm test` + `cargo test` (skipped `frontend/dist`-dependent test gets a dist stub) before merge.

## Open Questions

- Q1: Do we need a backend `GET /api/library/folders` for libraries that will never fetch the full track list on the folders view? If you anticipate > 200k tracks or extremely deep trees, answer is yes — add it as U0b. For now the answer is no; the tree built from `tracks` is the simpler, offline-friendly path.
- Q2: Should folder breadth be shown as counts alone or as "X tracks (+ Y subfolders)"? Recommendation: `N tracks` for leaf folders, `N tracks · M subfolders` when `children.size>0` to avoid surprise recursion on play.
- Q3: Should hidden/dot-directories be hidden from the browse even when they contain tracks (scanned despite dotfile)? No — if the scanner kept it, the folder view should show it; the scanner's `.mpdignore` is the exclusion mechanism.
