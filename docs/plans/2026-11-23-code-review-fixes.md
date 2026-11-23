# Code review fixes

Review baseline: `32abc782594238aa254232e142e1a9607b9beb84`.
Scope: all 27 findings from the full delegation review (4 high, 23 medium).
Implemented: R01–R04 (all high severity) and R11, which shared the R04 database migration. The remaining 22 medium findings are planned in the batches below.

## Working rules and test boundaries

- Add and run a failing regression before each fix; keep the regression in the repository.
- Exercise the existing installer entry points and generated kiosk launcher, the analyzer's capture lifecycle, and the scanner/database interfaces used during review.
- Use temporary directories, in-memory SQLite, and fake external commands/devices. Installer tests must never edit host mounts, `/etc`, or installed services.
- Preserve database row IDs, covers, album tags and source associations during migration. Track identity must distinguish different backing files while keeping rescans stable.
- Build the frontend before the complete backend suite. Run frontend checks with Node 26.
- Do not operate the live player, publish releases or commit changes as part of these fixes.

## Phase 1: high severity

| ID  | Finding                                    | Implementation                                                                                                                                                                                   | Regression / completion check                                                                                                                                                                                                    | Status                         |
| --- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------ |
| R01 | Uninstall removes user music mounts        | Mark only installer-created bind entries; remove that exact owned entry on uninstall. Leave pre-existing filesystem and bind mounts intact. Protect an existing mount during bind setup as well. | Temporary fstab fixtures cover an ordinary ext4 mount, an unowned bind, an owned bind, unrelated entries, repeat setup/uninstall and failed unmount. Observe external mount commands without executing them.                     | Implemented; local checks pass |
| R02 | Full kiosk URL is parsed as shell commands | Carry the kiosk URL through shell positional arguments and quote it at Chromium execution. Extract generated launcher rendering only as needed to test its actual output.                        | Run the generated launcher with fake cage/browser commands and a temporary Wayland fixture; assert full `panel=1&idle=...` URL arrives as one argument and the session supervises one browser. Cover full/viz modes.             | Implemented; local checks pass |
| R03 | CPAL stream drops after startup            | Give the capture worker explicit ownership of the stream for the analyzer lifetime. Create device resources on that worker; report startup errors and release resources when the analyzer stops. | Inject a fake capture resource at the external device boundary, then observe continued capture after initialization and release after shutdown. Cover startup failure and disabled/FIFO paths.                                   | Implemented; local checks pass |
| R04 | Different library sources overwrite tracks | Migrate uniqueness from relative URI to backing-path/CUE identity. Scope incremental mtime checks and CUE referenced/pruned sets to backing files. Refuse ambiguous URI-only playback lookups.   | Scan two roots with identical relative filenames and mtimes; both remain retrievable/playable. Cover CUE/plain collisions across roots, repeated scans, source removal, stable IDs, legacy database migration and FTS integrity. | Implemented; local checks pass |

## Phase 2: persistence and filesystem boundaries

Apply these after Phase 1. R11 must be checked during R04 migration work because the table/index migration can affect its failure mode.

| ID  | Finding                                        | Fix and required regression                                                                                                                                                                                                    | Status                         |
| --- | ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------ |
| R08 | Album-tag paths decoded twice / Unicode panic  | Use Axum's decoded path. API round trips must preserve literal percent sequences, Unicode and embedded slashes in album/key/value fields.                                                                                      | Planned                        |
| R09 | Cover paths escape library roots               | Canonicalize roots and the selected image, reject traversal and escaping symlinks, and restrict cache-cover keys. Serve inside-root images; return 404 for outside targets through both cover endpoints.                       | Planned                        |
| R10 | Punctuation breaks FTS queries                 | Quote/escape literal search terms using index-compatible tokenization. Search artist/title values containing slash, hyphen, quotes and Unicode without interpreting them as FTS operators.                                     | Planned                        |
| R11 | Existing libraries do not get their FTS index  | Detect missing/new index state and rebuild transactionally. Upgrade a populated legacy database and confirm both listings and MATCH queries find its rows; repeated migration remains safe.                                    | Implemented; local checks pass |
| R12 | Failed preset update deletes old configuration | Write/validate a replacement before retiring the old filename. Failure injection must preserve the existing readable preset for same-name and rename updates.                                                                  | Planned                        |
| R13 | Sanitized preset names collide                 | Reject creation/renaming into an existing distinct preset, or introduce stable IDs consistently through API/UI. Test `USB DAC` versus `USB-DAC`, reconnect/upsert callers and deletion by identity.                            | Planned                        |
| R14 | MPD string values are not escaped              | Serialize all MPD string literals and reject control characters. Quotes/backslashes must round trip; malformed values must fail before a fragment write or MPD restart.                                                        | Planned                        |
| R21 | Installer update removes custom Samba shares   | Generate shares from persisted library sources. Reinstall/update must preserve extra shares and keep a deleted default source absent. Isolate Samba commands in fixtures.                                                      | Planned                        |
| R26 | Failed radio persistence changes memory        | Persist station mutations before publishing their in-memory result, under one serialized mutation boundary. Add/update/delete write failures must preserve memory and disk; concurrent operations must not clobber each other. | Planned                        |

## Phase 3: DSP, playback and UI state

| ID  | Finding                                        | Fix and required regression                                                                                                                                                                                          | Status  |
| --- | ---------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- |
| R05 | Active DSP target is diagnosed inactive        | Derive diagnostic activity from direct or confirmed DSP routing. After enabling DSP, the selected target must retain its editable EQ controls.                                                                       | Planned |
| R06 | New DSP profile loses its first edit           | Keep unsaved default profile identity stable for the selected output. First edit and subsequent edits survive parent rerenders; deliberate selection changes retain the unsaved-edit decision.                       | Planned |
| R07 | CUE timing subtracts offsets twice             | Distinguish virtual-track timing from full-file timing; reuse the scanner's split duration. Assert last-track duration, elapsed display and seek positions for both playback forms.                                  | Planned |
| R15 | Disabled APO filters become active             | Skip OFF filters (or carry enabled state consistently). Imported disabled boosts must never reach the rendered EQ pipeline; ON filters and preamp still work.                                                        | Planned |
| R16 | Discovery stays active after timeout           | Report task completion as inactive and handle cancellation/restart consistently. A completed discovery must stop frontend polling without a manual cancel.                                                           | Planned |
| R17 | Zero enabled outputs reported healthy          | Return zero-output before healthy/paused/stopped whenever no effective listening destination exists. Cover configured-but-disabled and DSP-active routes.                                                            | Planned |
| R18 | Seek arrows also skip tracks                   | Consume handled slider key events before global shortcuts. Assert seek changes position without next/previous, while unfocused transport shortcuts still work.                                                       | Planned |
| R19 | Kiosk album action never opens library         | Navigate across the kiosk/full-page boundary or model it reactively. Clicking the album opens its detail; browser back returns correctly.                                                                            | Planned |
| R20 | Search/playlist album links use wrong identity | Resolve every album action to the library's source-aware folder identity, including MPD absolute/CUE URIs; close search when navigating. Test a title that differs from its folder key and multiple sources.         | Planned |
| R25 | Disabling DSP leaves manager active state      | Clear persisted and in-memory route state together and release/reconfigure the device boundary safely. A subsequent daemon failure must not restore a disabled route; failure handling must preserve a usable route. | Planned |

## Phase 4: release, development and test reliability

| ID  | Finding                                      | Fix and required regression                                                                                                                                                              | Status  |
| --- | -------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- |
| R22 | Manual tag builds publish another revision   | Check out the requested tag and validate it before building. A workflow/config regression must ensure the build ref and upload tag agree for dispatch and release-please calls.          | Planned |
| R23 | Fresh development startup serves wrong dist  | Resolve defaults against the workspace layout or give dev.sh an explicit startup root/config. A clean temporary checkout must serve built UI assets without an ignored local config.     | Planned |
| R24 | State tests mutate real radio data           | Give each test state a retained temporary runtime root and seed its own stations. Run against a sentinel pre-existing runtime store and assert that it remains unchanged.                | Planned |
| R27 | Smoke script searches for removed kiosk link | Update role/name and destination assertions to the current UI; open advanced settings where needed. Run the kiosk-return check against the rendered Back button using a fixture library. | Planned |

## Verification and finish criteria

1. Each finding has a reproducing regression that fails before its implementation and passes after it.
2. Installer fixtures pass without modifying host files, mounts or services. Generated shell scripts pass syntax checks.
3. `npm run build` and `npm test` pass using Node 26; record any pre-existing or intermittent failures separately.
4. `cargo test` passes with frontend/dist present. Database migration tests include populated legacy schemas, stable identities and searchable rows.
5. Review the final diff for interaction between database identity, CUE addressing, DSP diagnostics and navigation.
6. Update this plan with completed items, verification evidence and any hardware-only checks still pending. Live Linux hardware verification is separate from local automated checks.

## Implementation and validation record

- R01: `tests/install-music-bind.sh` failed before the fix by observing an unowned volume unmount and removed fstab entry. It now passes with ownership markers, mount identity verification, failed-unmount protection, and repeat setup/uninstall checks.
- R02: `tests/install-kiosk-launcher.py` failed before the fix because the full URL reached the fake browser incorrectly. Both full/viz modes now pass; the complete URL is one argument and the generated session preserves the browser exit code. Bash substitution also preserves literal ampersands.
- R03: the capture lifecycle regression failed because its fake device resource was dropped on startup return. It now receives a signal-bearing frame, retains the resource across analyzer clones, and releases it and the publisher on final drop. Disabled capture and startup errors are covered; existing FIFO decode tests pass.
- R04: the actual two-root WAV scan failed with one stored track instead of two. The CUE fixture then reproduced cross-source plain-row deletion, and the API fixture reproduced arbitrary source selection. Backing-path/CUE keys, stable upserts, scoped mtime/reference/cleanup sets, and ambiguity checks now pass these regressions.
- R11: populated legacy rows failed a real FTS MATCH search before the fix. The transactional upgrade preserves row IDs, source metadata, covers, CUE boundaries, and album tags; both new and already-present FTS indexes are checked, rebuilt when necessary, and remain searchable after upsert and repeat migration.
- Installer regressions are wired into `.github/workflows/ci.yml`; generated scripts pass shell syntax checks. Existing config-preservation, release-asset, permission-failure and MPD-config fixtures also pass.
- Frontend: Node 26 build passed; 20 test files / 148 tests passed. The visualization-settings failure observed during review did not recur in this run.
- Backend: 196 unit tests and 4 static-asset tests passed in an isolated repository copy. The copy resolved existing manifest/lockfile drift; the working-tree lockfile was not changed.

### Remaining verification

Local tests use fake mount/compositor/browser commands and a fake capture resource. Actual Linux mount integration, Chromium/cage on the HDMI panel, and CPAL capture on the player still require hardware verification before deployment. No live-player changes or commits have been made. Existing multi-root CUE playback still depends on how MPD exposes each library root; this change fixes scan/storage identity and rejects ambiguous URI-only requests.
