---
title: Lightweight WebKit Kiosk with Choosable Full/Viz Mode - Plan
type: feat
date: 2026-08-27
topic: kiosk-surf-viz
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-brainstorm
execution: code
---

# Lightweight WebKit Kiosk with Choosable Full/Viz Mode - Plan

## Goal Capsule

- **Objective:** The HDMI kiosk renders with a lightweight WebKit engine at roughly one-third the RAM of the current Chromium setup, and the operator can choose between the full now-playing player and an animation-only visualizer.
- **Means:** Replace Chromium with `surf` (webkit2gtk) under `cage`; bake a choosable content mode (`full` → `/kiosk`, `viz` → `/viz.html`) into the surf launch command as an installer template value.
- **Product authority:** This plan owns the kiosk display renderer and its content-mode selection. DSP, library, Bluetooth, and radio remain out of scope.
- **Execution:** code.
- **Open blockers:** None that block planning; the `surf`-under-`cage` Wayland path (`GDK_BACKEND=wayland`) is verified at build time, not before.

---

## Product Contract

### Summary

Swap the Chromium-based HDMI kiosk for a `surf` (webkit2gtk) session that renders either the existing full now-playing screen or a new animation-only visualizer page, selected at install time. RAM drops from ~300–450 MB (Chromium) to ~80–130 MB (surf/webkit) under either mode.

### Problem Frame

The kiosk host runs a single-app Chromium session to show the player wall. Chromium's resident footprint (~300–450 MB) is the dominant memory cost on the device and, per the user, too high. The expensive audio analysis already runs server-side in `VisualizerAnalyzer` and is broadcast over `/api/visualizer`; the browser only paints it. The prior `docs/plans/2026-08-23-1432-feat-hdmi-kiosk-display-plan.md` established the `cage` + Chromium session and `write_kiosk()` installer path this plan now changes.

### Key Decisions

- **Engine is `surf` (webkit2gtk), not Chromium, a native renderer, or hardware LEDs.** (session-settled: user-directed — chosen over native Wayland/wgpu and LED options: user wants minimal RAM with the least implementation effort and is willing to keep a browser engine.) Governs R1, R2, R3.
- **Content mode is choosable between `full` and `viz`, default `viz`.** (session-settled: user-directed — chosen over forcing a single scope: user wants both available, with animation-only as the minimal-RAM default.) Governs R4, R5, R6.
- **Mode is a baked installer template value, not a backend config key.** (session-settled: user-approved — follows the existing kiosk launch-URL convention where the URL is baked at install, not read from backend config.) Governs R6.

### Requirements

**Engine replacement**

- R1. The kiosk graphical session launches `surf` under `cage` instead of Chromium.
- R2. `install.sh`'s `write_kiosk()` installs `surf` (pulling webkit2gtk) and removes the Chromium package-resolution hack (empty `chromium-browser` package plus snap fallback).
- R3. The existing session fixes are retained: `cage` single-app compositor, `loginctl enable-linger oxide`, `ExecStartPre=-/usr/bin/chvt 7`, and the PAM `class=user` unit — these fix the graphical session, not Chromium.

**Content mode**

- R4. The kiosk content mode is choosable between `full` (existing now-playing screen) and `viz` (animation-only visualizer); the choice is made at install time.
- R5. The selected mode determines the URL `surf` loads: `full` → `http://127.0.0.1__PORT_SUFFIX__/kiosk?panel=1&idle=__KIOSK_IDLE_SECONDS__`; `viz` → `http://127.0.0.1__PORT_SUFFIX__/viz.html` (the `__PORT_SUFFIX__` token tracks `LISTEN`, default `:80`).
- R6. The mode is baked as an installer template value into the surf launch command (per Key Decision 3), so the mode is selectable without a backend config key or a frontend rebuild.

**Animation-only page (`viz`)**

- R7. A new standalone `frontend/public/viz.html` is added: self-contained vanilla JS (no React/Vite bundle), opens `ws://<host>/api/visualizer`, and draws the visualizer (bars and/or ring) on a fullscreen canvas via `requestAnimationFrame`.
- R8. `viz.html` reuses the look from `frontend/src/components/Visualizer.tsx` (`DEFAULT_VIZ_PARAMS`, emerald/indigo accents) and fades to black when the stream `level` stays ≈ 0.
- R9. The backend serves `viz.html` from `frontend/dist` (Vite copies `public/` verbatim) at `http://127.0.0.1__PORT_SUFFIX__/viz.html`.

**Full-player mode (`full`)**

- R10. `full` mode reuses the existing `/kiosk` route and `KioskView` with no frontend changes; `surf` displays it as-is (the `?panel=1&idle=` query arms panel auto-return, unchanged from the current session).

**Memory target**

- R11. Renderer resident RAM is ≤ ~130 MB under either mode, down from the ~300–450 MB Chromium baseline.

### Key Flows

- F1. Install-time mode selection
  - **Trigger:** `write_kiosk()` runs during install.
  - **Actors:** installer, operator (chooses mode).
  - **Steps:** operator selects `full` or `viz`; installer sets the surf launch URL accordingly and writes the kiosk unit.
  - **Covers R2, R4, R5, R6.**
- F2. Kiosk runtime render
  - **Trigger:** systemd starts the kiosk unit after `loginctl enable-linger`.
  - **Actors:** `cage`, `surf`, backend `/api/visualizer` (and `/kiosk` for `full`).
  - **Steps:** `cage` opens a Wayland session; `surf` loads the mode URL; the page connects the WS and paints frames; a `surf` crash triggers systemd restart.
  - **Covers R1, R3, R5, R7, R10.**
- F3. Mode switch after deploy
  - **Trigger:** operator wants the other mode.
  - **Steps:** operator changes the mode by re-running the installer with a different `KIOSK_MODE`; directly editing the baked unit URL is a manual workaround, and a clean post-install switch is deferred (see Outstanding Questions). No backend or frontend rebuild is required.
  - **Covers R6.**

### Acceptance Examples

- AE1. Given mode `viz`, when the kiosk starts, then `surf` loads `/viz.html`, the canvas animates from `/api/visualizer` frames, and no React bundle is fetched.
  - **Covers R5, R7, R9.**
- AE2. Given mode `full`, when the kiosk starts, then `surf` loads `/kiosk` (with `?panel=1&idle=`) and shows the existing now-playing UI unchanged.
  - **Covers R5, R10.**
- AE3. Given mode `viz` and no audio, when `level` stays ≈ 0, then the canvas fades to black.
  - **Covers R8.**
- AE4. Given either mode, when `surf` exits abnormally, then systemd restarts it within the unit's restart window.
  - **Covers R1, R3.**
- AE5. Given either mode running, when measured, then renderer resident RAM is ≤ ~130 MB.
  - **Covers R11.**

### Scope Boundaries

- Deferred for later: fetching the server's persisted `VizParams` (`vizparams.json`) into `viz.html`; touch/transport controls in `viz` mode (animation-only by design); a hardware-LED alternative.
- Outside this product's identity: a native Rust renderer (Wayland/wgpu or raw framebuffer) that removes the browser engine entirely; making the mode a backend config key instead of a baked installer value.

### Dependencies / Assumptions

- `surf` is available in the target Ubuntu repos and links webkit2gtk with a working Wayland backend under `cage` (`GDK_BACKEND=wayland`) — verified at build, not before planning.
- The backend `/api/visualizer` WebSocket exists and emits `SpectrumFrame { bins: [72] f32, level: f32 }` at ~40 fps when the visualizer capture path is enabled (`backend/src/visualizer/mod.rs`); on a clean install the capture is off, so `viz.html` stays black until enabled.
- Vite copies `frontend/public/` into `frontend/dist/` verbatim, so `viz.html` is served without a route change.
- The backend binds `LISTEN` (default `0.0.0.0:80`); the kiosk URL is built from `127.0.0.1__PORT_SUFFIX__` so it tracks a non-default `LISTEN` (the installer already computes `__PORT_SUFFIX__`).

<!-- ce-section: work-relationships -->
### How This Work Fits Together

This plan owns the kiosk renderer swap (Chromium → surf) and the choosable content mode. It supersedes the engine portion of `docs/plans/2026-08-23-1432-feat-hdmi-kiosk-display-plan.md` (that plan's `cage` session, `write_kiosk()` structure, and PAM/chvt fixes are retained and extended here).

- Shares authority with `backend/src/visualizer/mod.rs` (the `SpectrumFrame` contract `viz.html` consumes).
- Shares authority with `frontend/src/components/Visualizer.tsx` (`DEFAULT_VIZ_PARAMS` / draw routines ported into `viz.html`).
- Reuses `frontend/src/components/KioskView.tsx` unchanged for `full` mode.
- Enables, independently of this plan: a future hardware-LED visualizer option; server-side `VizParams` fetch into `viz.html`.
- Still to decide (Deferred to Planning): exact `surf` fullscreen flag, whether mode is also exposed post-install without re-running the installer, and the `VizParams` fetch approach.

### Outstanding Questions

- Resolve Before Planning: none.
- Deferred to Planning:
  - Exact `surf` fullscreen invocation (e.g., `-F` flag behavior) and whether `GDK_BACKEND=wayland` is required under `cage`.
  - Whether the mode should also be changeable post-install without re-running the installer (e.g., a small read-by-unit env/file), or strictly a baked template value. F3 notes re-running the installer as the supported switch; a clean post-install switch remains deferred.

### Sources / Research

- `backend/src/visualizer/mod.rs` — `BANDS = 72`, `PUBLISH_HZ = 40.0`, `SpectrumFrame`, `VizParams` persisted to `vizparams.json`.
- `frontend/src/components/Visualizer.tsx` — `DEFAULT_VIZ_PARAMS`, `drawBars` / `drawRing` / `drawCircular`, `VizStyle`.
- `frontend/src/components/KioskView.tsx` — existing `/kiosk` now-playing screen (reused in `full` mode).
- `install.sh` — `write_kiosk()` kiosk installer path (Chromium resolution hack to remove; cage/PAM/chvt fixes to retain); session template builds the kiosk URL as `http://127.0.0.1__PORT_SUFFIX__/kiosk?panel=1&idle=__KIOSK_IDLE_SECONDS__`.
- `docs/plans/2026-08-23-1432-feat-hdmi-kiosk-display-plan.md` — prior cage + Chromium kiosk plan this plan supersedes the engine of.

---

## Planning Contract

Product Contract preserved unchanged from the requirements-only source.

### Key Technical Decisions

- KTD1. `surf` launches with `GDK_BACKEND=wayland` inside the `oxide-kiosk-session` script so webkit2gtk uses the Wayland backend and finds the `cage` socket. This is an assumption verified at build (Deferred to Planning), not a settled runtime guarantee.
- KTD2. The content mode is baked as an installer template value `KIOSK_MODE` substituted into the session script's launch URL. (session-settled: user-approved — chosen over a backend config key: follows the existing kiosk launch-URL convention where the URL is baked at install.) Governs R6.
- KTD3. Compositor-level blanking (`swayidle` / `wlopm` + `oxide-kiosk-idle-watcher`) is unchanged and keys off playback status, independent of the page; the in-page idle fade in `viz` mode is cosmetic only. Governs R8.
- KTD4. Default `KIOSK_MODE=viz` on a fresh install. (session-settled: user-directed — chosen over `full`: preserves the minimal-RAM intent while keeping the full player one flag away.) Governs R4, R5.

### Assumptions

- `surf` (webkit2gtk) is installable from the target Ubuntu repos and renders a canvas + WebSocket page under `cage`; if the Wayland backend misbehaves, the fallback is a `webkit2gtk` headless launcher, still not Chromium.
- `KIOSK_ENABLED` stays opt-in (default 0); mode selection only matters when the kiosk is enabled.
- No backend change is required: `viz.html` is served as a static asset and `/kiosk` already exists.
- `viz.html` only animates when the visualizer capture path is enabled (`visualizer_fft` + a capture device/fifo); on the target hardware the installer configures this, but a clean install without capture shows a black page until enabled.

---

## Implementation Units

### U1. Add animation-only visualizer page

- **Goal:** Ship a standalone `viz.html` that paints the visualizer from `/api/visualizer` with no React bundle.
- **Requirements:** R7, R8, R9.
- **Dependencies:** none.
- **Files:** `frontend/public/viz.html`.
- **Approach:**
  1. Write a self-contained HTML page (inline CSS + vanilla JS, no module imports).
  2. Open `new WebSocket('ws://' + location.host + '/api/visualizer')`; on each `SpectrumFrame` parse `bins` and `level`.
  3. Draw bars (and/or ring) on a fullscreen `<canvas>` via `requestAnimationFrame`, reusing the `DEFAULT_VIZ_PARAMS` palette (emerald/indigo accents) from `frontend/src/components/Visualizer.tsx`.
  4. Track a silence counter; when `level` stays ≈ 0 for a short threshold, fade canvas opacity to 0 (cosmetic idle), restore on activity.
  5. Add a WebSocket reconnect loop so a dropped frame stream resumes automatically.
- **Patterns to follow:** `frontend/src/components/Visualizer.tsx` — port the look and palette as plain canvas calls; do not import React or the Vite bundle.
- **Test scenarios:**
  - Covers AE1. Given the page loads and the WS delivers frames, the canvas animates and no React/Vite chunk is requested.
  - Covers AE3. Given `level` ≈ 0 for > 2 s, the canvas opacity fades toward 0.
  - Edge: WS drops and reconnects — the page reopens the socket and resumes without a reload.
- **Verification:** `cd frontend && npm run build` emits `dist/viz.html`; `GET http://127.0.0.1__PORT_SUFFIX__/viz.html` returns 200 (on a default install that is port 80); a browser smoke shows animated bars against a running backend with active audio and the visualizer capture enabled.

### U2. Rework `write_kiosk()` to launch `surf`

- **Goal:** Replace the Chromium install and binary-resolution path with `surf`.
- **Requirements:** R1, R2, R3.
- **Dependencies:** none.
- **Files:** `install.sh` (`write_kiosk()`, ~1190–1386; inline `oxide-kiosk.service` template ~1395–1422).
- **Approach:**
  1. Change the `apt_install` line from `cage swayidle wlopm chromium-browser` to `cage swayidle wlopm surf`.
  2. Delete the chromium-browser candidate loop and the `snap install chromium` fallback block.
  3. In the `oxide-kiosk-session` template, replace the Chromium launch (`cage -d -s -- __BROWSER__ --ozone-platform=wayland --kiosk ...`) with `cage -d -s -- surf -F __KIOSK_URL__`, and add `export GDK_BACKEND=wayland` to the session env.
  4. Retain the PAM `class=user` unit, `ExecStartPre=-/usr/bin/chvt 7`, and `loginctl enable-linger` logic unchanged.
- **Patterns to follow:** Keep the existing fault isolation (warn and return 0 if packages are unavailable; `KIOSK_ENABLED` opt-in gate) so non-kiosk installs are unaffected.
- **Test scenarios:**
  - Covers AE4 (partial). Given `KIOSK_ENABLED=1` on a host with `surf`, `write_kiosk` installs `surf` and writes a session that execs `surf`; no `chromium` package is pulled.
  - Given kiosk packages unavailable, `write_kiosk` warns and returns 0 (playback unaffected).
- **Verification:** Run the installer in a VM/container; `command -v surf` resolves; the generated `oxide-kiosk-session` contains `surf -F` and contains no `chromium` reference.

### U3. Make kiosk content mode choosable and bake it

- **Goal:** Let the operator pick `full` or `viz` at install; bake the chosen URL into the session.
- **Requirements:** R4, R5, R6.
- **Dependencies:** U2.
- **Files:** `install.sh` (add `KIOSK_MODE` var + default `viz`; map to URL; substitute `__KIOSK_URL__`), `contrib/systemd/oxide-kiosk.service` (doc comment kept in sync).
- **Approach:**
  1. Add `KIOSK_MODE` (full|viz, default `viz` per KTD4); validate it against the two values and reject/ default otherwise.
  2. Map `full` → `http://127.0.0.1__PORT_SUFFIX__/kiosk?panel=1&idle=__KIOSK_IDLE_SECONDS__`, `viz` → `http://127.0.0.1__PORT_SUFFIX__/viz.html`.
  3. Substitute `__KIOSK_URL__` (replacing the previously baked `/kiosk` URL) into the session template, using the same baked-template + sed/`${var}` substitution convention the installer already applies (the mechanism formerly used for the `__BROWSER__` token).
  4. Keep `KIOSK_ENABLED` opt-in; mode only applies when the kiosk is enabled.
- **Patterns to follow:** Reuse the existing baked-template + sed/`${var}` substitution convention the installer already applies (the mechanism formerly used for the `__BROWSER__` token).
- **Test scenarios:**
  - Covers AE2. Given `KIOSK_MODE=full`, the generated session loads `/kiosk?panel=1&idle=`.
  - Covers AE1. Given `KIOSK_MODE=viz` (default), the generated session loads `/viz.html`.
  - Given an unrecognized `KIOSK_MODE`, the installer rejects or defaults to `viz`.
- **Verification:** Inspect the generated `oxide-kiosk-session` for the expected URL per mode; re-running the installer is idempotent.

### U4. Live-server smoke and memory verification

- **Goal:** Confirm the `surf` kiosk runs on target hardware and meets the RAM target.
- **Requirements:** R11, R1, R3, R8.
- **Dependencies:** U1, U2, U3.
- **Files:** (verification only) deploy to `192.168.100.104`; no repository file changes.
- **Approach:**
  1. Build the frontend (emits `dist/viz.html`) and deploy backend + frontend to the target.
  2. Re-run `install.sh` with `KIOSK_ENABLED=1` (default `viz`), or edit the baked URL for `full`; `systemctl restart oxide-kiosk`.
  3. Measure renderer RSS (`surf` + `cage`) via `ps`/`systemd` and confirm ≤ ~130 MB.
  4. Confirm crash-restart (kill `surf` → systemd restarts the session) and compositor blanking (stop playback → panel blanks via `wlopm`; touch wakes).
- **Execution note:** This is packaging/runtime verification; prefer an install + live smoke over unit coverage.
- **Test expectation:** none — runtime smoke verification per the Execution note.
- **Verification:** `surf` process visible on the panel; RSS within target; blanking and crash-restart observed on the live HDMI display.

---

## Verification Contract

- **Frontend build:** `cd frontend && npm run build` — emits `dist/viz.html` alongside the existing bundle.
- **Static serve:** `GET http://127.0.0.1__PORT_SUFFIX__/viz.html` → 200; `GET http://127.0.0.1__PORT_SUFFIX__/kiosk` → 200 (on a default install that is port 80; the `__PORT_SUFFIX__` token tracks `LISTEN`).
- **Animation smoke:** load `/viz.html` against a running backend with active audio and the visualizer capture enabled; the canvas animates from WS frames (browser smoke, no automated unit test).
- **Installer:** clean VM/container install with `KIOSK_ENABLED=1` — `surf` present, session execs `surf -F <url>`, no `chromium` package installed.
- **Live deploy (target `192.168.100.104`):** re-run `install.sh` `KIOSK_ENABLED=1`; `systemctl restart oxide-kiosk`; measure `surf` RSS (e.g. `ps -o rss= -C surf`) ≤ ~130 MB; kill `surf` → session restarts; stop playback → panel blanks via `wlopm`; touch wakes.
- No automated unit tests for the page; UI smoke and installer verification cover it.

---

## Definition of Done

- **Global:** `surf` kiosk renders either mode at ≤ ~130 MB; Chromium is fully removed from the kiosk path; `cage`/PAM/chvt/linger fixes are retained; compositor blanking is intact.
- **U1:** `dist/viz.html` exists and animates from `/api/visualizer`; no React bundle is fetched.
- **U2:** installer installs `surf`, writes a `surf`-launching session, and pulls no `chromium` packages.
- **U3:** `KIOSK_MODE` selects the URL baked into the session; an invalid value is rejected or defaulted to `viz`.
- **U4:** live kiosk runs within the RAM target; crash-restart and blanking are verified on hardware.
- **Cleanup:** no leftover `chromium` references, snap-fallback code, or dead `__BROWSER__` scaffolding remain in `install.sh`.
