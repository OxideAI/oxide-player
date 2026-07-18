# Plan — Live audio visualizer for Kiosk mode (issue #6)

**Type:** feat
**Origin:** docs/brainstorms/2026-07-17-kiosk-visualizer-requirements.md

## Problem frame
The Kiosk view (`frontend/src/components/KioskView.tsx`) shows a static album cover
with a purely decorative CSS `.eq` animation only when no cover is present. We want a
reactive, full-screen animation that visually tracks playback. Because audio is produced
by MPD on the server (never streamed to the browser), the Web Audio `AnalyserNode` path
is not viable. The accepted approach (Option B) is a procedural, frontend-only visualizer
driven by signals the frontend already polls.

## Scope boundary
- In scope: a new `Visualizer` component, rendered as a full-bleed blurred backdrop behind
  `.stage` in Kiosk view; replaces the static `.eq` bars with a lightweight version.
- Out of scope: real FFT from the server (Option A), persisted settings / style picker
  (nice-to-have, deferred), any backend or config change.

## Key Technical Decisions
- **Procedural motion, not real spectrum.** Bar heights come from layered sines/multiplicative
  noise seeded by `elapsed` so the field evolves over time and pulses on beat-like intervals.
  A radial bloom/particle layer intensifies with `volume`. (see origin: requirements)
- **Drive from polled status.** Component reads `playing`, `elapsed`, `duration`, `volume`
  from `PlayerStatus`. Smoothing of elapsed already exists via `useSmoothElapsed` in KioskView;
  the visualizer consumes the same `status`/`elapsed`/`duration`/`volume` values.
- **Canvas + requestAnimationFrame.** Sized to `devicePixelRatio`. Loop starts on mount and
  when `playing` becomes true; stops (and is cancelled) when `!playing` and on unmount to
  avoid RAF leaks. Pass a continuously-advancing phase clock so motion is smooth even when
  `elapsed` only updates ~1/s.
- **`prefers-reduced-motion`.** Query via `window.matchMedia('(prefers-reduced-motion: reduce)')`;
  when set, render a single static gradient and never start the RAF loop.
- **Visual language.** Reuse CSS vars `--accent`, `--accent-2`, `--accent-soft`, `--ease`,
  `--r-xl`; glassy/audiophile aesthetic matching the existing `.artGlow`/`oxPulse` styling.
  Keep the backdrop blurred and low-contrast so the album art + meta text stay legible.

## Implementation Units

### U1 — Visualizer component skeleton + canvas loop
Create `frontend/src/components/Visualizer.tsx` and `Visualizer.module.css`.
Props: `playing: boolean`, `elapsed: number`, `duration: number`, `volume: number`.
- `useEffect`-managed `requestAnimationFrame` loop with a `phaseRef` clock.
- A `ResizeObserver` (or window resize + dpr) sizes the canvas; account for `devicePixelRatio`.
- Start loop when `playing` is true; cancel it when `playing` is false and on unmount
  (`cancelAnimationFrame` in cleanup).
- Honors `prefers-reduced-motion`: if reduced, paint a static gradient once and skip the loop.
**Files:** `frontend/src/components/Visualizer.tsx`, `frontend/src/components/Visualizer.module.css`

### U2 — Procedural bar spectrum + bloom render
Draw within the RAF loop:
- A frequency-bar spectrum across the width; bar heights = `base + sum(sin(phase*f + i)*amp)`,
  modulated by volume and a slow beat-pulse envelope derived from `phase`.
- A radial bloom (gradient circle) whose radius/alpha scale with `volume`.
- When `!playing`, freeze the last frame (loop already stopped) — no motion.
**Files:** `frontend/src/components/Visualizer.tsx`

### U3 — Mount Visualizer in KioskView behind `.stage`
In `frontend/src/components/KioskView.tsx`, render `<Visualizer playing={playing}
elapsed={smoothElapsed} duration={duration} volume={status?.volume ?? 0} />` as a
full-bleed layer behind `.stage` (absolutely positioned, blurred, `pointer-events: none`,
`z-index` below `.stage`). Remove the static `.eq` decorative bars (keep `.note`/empty
art styling minimal). Pass `playing`, `elapsed`, `duration`, `volume` already available.
**Files:** `frontend/src/components/KioskView.tsx`, `frontend/src/components/KioskView.module.css`

## Existing patterns to follow
- `useSmoothElapsed`, `useDragValue` in `frontend/src/components/playerHooks.ts` (smoothing).
- `KioskView.module.css` `.artGlow` / `@keyframes oxPulse` for the bloom aesthetic and var usage.
- CSS Modules only; no Tailwind (AGENTS.md / skill hard rule).
- `PlayerStatus` shape in `frontend/src/types.ts` (`state`, `volume`, `elapsed`, `duration`).

## Test scenarios
This is a UI-only feature; coverage is type-check + manual visual verification.
- `cd frontend && npm run build` passes type-check (acceptance criterion).
- Manual: Kiosk view shows moving animation while `state === 'playing'`; freezes when paused/stopped.
- Manual: `prefers-reduced-motion: reduce` shows a static gradient, no animation, no CPU loop.
- Manual: navigating away from Kiosk view (unmount) leaves no running RAF loop (no console growth
  of animation callbacks; check via DevTools performance or a quick `window.__rafCount` debug if needed).
- Manual: animation visible behind album art and controls without harming text contrast.

## Dependencies / sequencing
- U1 → U2 (render logic builds on the loop) → U3 (mount). No backend dependency.
- No new npm dependencies required (canvas + RAF are native).

## Risks
- **RAF leak** if cleanup is wrong — mitigated by `cancelAnimationFrame` in effect cleanup and
  gating the loop on `playing`.
- **Contrast** — backdrop must stay low-opacity/blurred; verify against light covers.
- **Reduced-motion** — must be checked at runtime (media query can change), not just initial.
