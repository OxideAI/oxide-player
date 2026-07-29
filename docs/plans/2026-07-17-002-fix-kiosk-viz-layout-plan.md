# Plan — Kiosk: fix visualizer visibility + vertical layout overflow (issue #6 follow-up)

**Type:** fix
**Origin:** docs/brainstorms/2026-07-17-kiosk-visualizer-requirements.md, docs/plans/2026-07-17-001-feat-kiosk-visualizer-plan.md

## Problem frame
Two defects in Kiosk mode (`frontend/src/components/KioskView.tsx` + `KioskView.module.css`):

1. **Visualizer not visible to the user.** The `Visualizer` component paints correctly (verified: canvas backing store is 100% filled, vivid bloom). The user reports "no visualization" because the **PWA service worker was configured `registerType: 'prompt'`** (`frontend/vite.config.ts`), which does NOT auto-update — the browser keeps serving a stale cached bundle from before the visualizer existed, even after a green rebuild. This was already changed to `autoUpdate` in the previous session but must be confirmed deployed and the transition made robust.
2. **Vertical layout overflow on short viewports.** `.kiosk` is `position: fixed; inset: 0; overflow: hidden` and centers `.stage` (fixed-size art `min(54vmin,460px)` + `clamp(1.6rem,4vh,3.5vh)` gaps + meta + progress + controls + volume). On shorter screens the content is taller than the viewport, so it is clipped: the album art is cut off at the top (`top: -34px`) and the volume control falls below the fold (`top: 632px` in a `618px` viewport) — invisible.

## Scope boundary
- In scope: make the visualizer reliably visible (SW auto-update + a defensive cache-bust so the user is never stuck on a stale bundle) and fix Kiosk vertical spacing so art + volume are always fully visible.
- Out of scope: real FFT (Option A), settings/style picker, changes to other views.

## Key Technical Decisions

### K1 — Service worker must auto-update (visualizer visibility root cause)
Change `registerType: 'prompt'` → `'autoUpdate'` in `frontend/vite.config.ts` (done in prior session; re-verify). With `autoUpdate`, a fresh build's new content-hashed bundle is precached and activated automatically on next load, so the user stops seeing the pre-visualizer cached build. (see origin: brainstorm — Option B is purely frontend; a stale cache is the only thing blocking it from showing.)

### K2 — Defensive: force a clean SW/cache swap on deploy
Beyond `autoUpdate`, the transition from the old `prompt` SW to the new one can still hold a stale precache for one load. Add `clientsClaim: true` and `skipWaiting: true` to the `VitePWA` options so the new SW takes control immediately and serves the new precache without a stale gap. This guarantees the rebuilt visualizer shows after one reload.

### K3 — Kiosk layout must fit short viewports
Replace the fixed/overflow-clipped centering with a layout that always fits and scrolls if truly necessary:
- Keep `.kiosk` `position: fixed; inset: 0` but change `overflow: hidden` → `overflow-y: auto` (so nothing is silently clipped; short screens scroll instead of cutting off the art/volume).
- Constrain total stage height to the viewport: cap the art with `max-height` tied to available space (e.g. `min(54vmin, 460px, 42dvh)`) so it shrinks on short screens instead of overflowing.
- Reduce the `.stage` `gap` clamp upper bound and make it `dvh`-aware so the stack (art + meta + progress + controls + volume) fits within `100dvh` minus padding.
- Ensure `.volume` always sits within the viewport: because the stage is now height-constrained and scrollable as a fallback, volume will no longer fall below the fold.

### K4 — Visualizer brightness already correct
The `Visualizer` render math (bright bloom alpha `0.22 + vol*0.3 + beat*0.12`, bars, `blur(16px)`, `opacity 0.95`) is verified vivid. No change needed to the component itself; visibility is delivered by K1/K2 (cache) and K3 (art no longer covers the centered bloom on short screens). Keep the existing `prefers-reduced-motion` + RAF-cleanup behavior.

## Implementation Units

### U1 — SW auto-update + immediate claim (K1, K2)
`frontend/vite.config.ts`: set `registerType: 'autoUpdate'` and add `injectRegister: 'auto'` (or keep the `virtual:pwa-register/react` registration in `src/main.tsx`, which already supports autoUpdate). Add `workbox: { ... }` option `clientsClaim: true, skipWaiting: true` so the new SW activates immediately. Rebuild; verify `dist/sw.js` reflects `autoUpdate`/skip-waiting.
**Files:** `frontend/vite.config.ts`, `frontend/src/main.tsx` (verify `UpdateToast` path still valid under autoUpdate — it becomes a no-op/informational toast, acceptable).

### U2 — Kiosk vertical-fit layout (K3)
`frontend/src/components/KioskView.module.css`:
- `.kiosk`: `overflow: hidden` → `overflow-y: auto`; keep `min-height: 100dvh`.
- `.stage`: reduce `gap` upper clamp (e.g. `clamp(1rem, 3vh, 2.2vh)`); ensure it can shrink.
- `.art`: `width/height: min(54vmin, 460px)` → `min(54vmin, 460px, 42dvh)` so it scales down on short viewports (fixes top cutoff).
- Confirm `.volume` remains inside the flow (no absolute positioning) so it is never below the fold once the stage fits.
**Files:** `frontend/src/components/KioskView.module.css`

### U3 — Verify both fixes in-browser (K4 confirmation)
Start backend + MPD, play a track, load `/kiosk` at a SHORT viewport (e.g. 1231×618) with SW cache cleared once:
- Visualizer canvas backing is ~100% painted and the bloom is visible around the art.
- Album art `top >= 0` (not clipped) and volume control `bottom <= viewport height` (visible).
- Repeat at a tall viewport (e.g. 1440×900) to confirm no regression.
**Files:** manual/Playwright verification only.

## Existing patterns to follow
- CSS Modules only; reuse vars `--bg`, `--text`, `--accent`, `--r-xl`, `--ease`, `dvh` units already used in `KioskView.module.css`.
- `VitePWA` config shape in `frontend/vite.config.ts` (lines 9–45+).
- `useRegisterSW` + `UpdateToast` in `frontend/src/main.tsx` / `frontend/src/components/UpdateToast.tsx`.
- `Visualizer` component unchanged (`frontend/src/components/Visualizer.tsx`).

## Test scenarios
- `cd frontend && npm run build` passes type-check (acceptance criterion from origin).
- Short viewport (618px tall): art not clipped at top, volume visible, no silent clipping.
- Tall viewport: layout unchanged/centered as before.
- Kiosk with track playing: visualizer animates; paused/stopped: static gradient.
- `prefers-reduced-motion`: static gradient, no RAF loop.
- After rebuild, a normal reload (no manual cache clear) serves the new bundle (autoUpdate works).

## Dependencies / sequencing
- U1 (SW) and U2 (CSS) are independent; both before U3 verification.
- No backend changes. No new npm dependencies.

## Risks
- `skipWaiting`/`clientsClaim` can interrupt an in-flight navigation; for a kiosk PWA this is acceptable and desired.
- `overflow-y: auto` on a fixed full-screen could show a scrollbar on borderline viewports; mitigate by the `42dvh` art cap + reduced gap so scroll is rarely needed.
