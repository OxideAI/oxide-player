# Requirements — Live audio visualizer for Kiosk mode (issue #6)

## Outcome
A reactive, frontend-only animation on the Kiosk full-screen "now playing" view
that visually tracks playback without any browser-side audio analysis.

## Approach
**Option B (procedural, recommended in the issue).** No backend changes, no new
config, works on every setup. The browser never receives the PCM stream
(audio is produced by MPD on the server), so the Web Audio `AnalyserNode` path
is not viable. We drive motion from signals the frontend already polls:
`PlayerStatus.state`, `elapsed`, `duration`, and `volume`.

## Hard constraints
- No Tailwind. CSS Modules only (`*.module.css`).
- Zero new backend code or config to use it.
- Option A (real FFT from server) is explicitly out of scope for this issue.

## Scope
- New `Visualizer` component (`frontend/src/components/Visualizer.tsx` +
  `Visualizer.module.css`).
- Rendered as a full-bleed, blurred backdrop behind `.stage` in
  `frontend/src/components/KioskView.tsx` so album art and controls float on top.
- Props: `playing`, `elapsed`, `duration`, `volume`, plus accent colors pulled
  from existing CSS vars (`--accent`, `--accent-2`, `--accent-soft`).
- `<canvas>` sized to `devicePixelRatio`; `requestAnimationFrame` loop.
- Visual design: layered-sine frequency bars whose heights evolve with `elapsed`
  and pulse on beat-like intervals, plus a soft radial bloom/particle field that
  intensifies with `volume`. Frozen/paused when `state !== 'playing'`.
- Replaces the static `.eq` decorative bars (shown only when no cover) with a
  lightweight version of the same canvas field.
- `prefers-reduced-motion`: fall back to a static gradient, no RAF loop.

## Non-goals
- Real spectrum analysis (Option A) — follow-up if scoped later.
- Persisted settings / style picker (nice-to-have, deferred).

## Acceptance criteria
- [ ] Kiosk view shows a live animation that moves while `state === 'playing'`
      and is static/paused otherwise.
- [ ] Purely frontend (Option B); no new required config.
- [ ] CSS Modules only; matches existing glassy/audiophile language.
- [ ] `prefers-reduced-motion` honored.
- [ ] No RAF/memory leaks (loop cleaned up on unmount and pause).
- [ ] `cd frontend && npm run build` passes type-check.

## Decisions resolved in dialogue
- Visualizer lives behind `.stage` as backdrop; keeps contrast for existing text.
- Default on while playing; no toggle in this issue.
