import { useEffect, useRef } from 'react'
import type { SpectrumFrame } from '../useVisualizer'
import styles from './Visualizer.module.css'

interface Props {
  playing: boolean
  frame: SpectrumFrame | null
  params?: VizParams
}

/// Live-tunable visualizer parameters. Exposed via the temporary dev controls
/// (sliders + copy button) so the look can be dialed in without recompiling.
export interface VizParams {
  bloomAlpha: number // base halo opacity (0..1)
  bloomBeat: number // halo opacity added by the beat pulse (0..1)
  bloomEnergy: number // halo opacity added by audio level (0..1)
  bloomRadius: number // halo radius as a multiple of the viewport diagonal
  barIdle: number // minimum bar height fraction at rest (0..1)
  barPeak: number // extra bar height fraction at full signal (0..1)
  barGap: number // pixels between bars
  barRadius: number // bar corner radius cap (px)
  phaseSpeed: number // idle-pulse tempo (higher = faster)
  blur: number // CSS blur in px
}

export const DEFAULT_VIZ_PARAMS: VizParams = {
  bloomAlpha: 0.42,
  bloomBeat: 0.22,
  bloomEnergy: 0.28,
  bloomRadius: 1.05,
  barIdle: 0.18,
  barPeak: 0.82,
  barGap: 3,
  barRadius: 6,
  phaseSpeed: 1.1,
  blur: 6,
}

// The number of bars follows the backend's published bin count (BANDS in
// visualizer/mod.rs) at runtime, so the two can never drift out of sync. We
// only need a stable upper bound for the smoothed-height buffer.
const MAX_BARS = 256

export function Visualizer({ playing, frame, params }: Props) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null)
  const rafRef = useRef<number | null>(null)
  const reducedRef = useRef(false)
  const frameRef = useRef<SpectrumFrame | null>(frame)
  frameRef.current = frame
  // Params read by the render loop without restarting it on every slider tick.
  const paramsRef = useRef<VizParams>(params ?? DEFAULT_VIZ_PARAMS)
  paramsRef.current = params ?? DEFAULT_VIZ_PARAMS

  useEffect(() => {
    const mq = window.matchMedia('(prefers-reduced-motion: reduce)')
    reducedRef.current = mq.matches
    const onMq = () => {
      reducedRef.current = mq.matches
    }
    mq.addEventListener('change', onMq)
    return () => mq.removeEventListener('change', onMq)
  }, [])

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return

    let accent = '#6ee7b7'
    let accent2 = '#8b9cff'
    const syncVars = () => {
      const s = getComputedStyle(canvas)
      accent = s.getPropertyValue('--accent').trim() || accent
      accent2 = s.getPropertyValue('--accent-2').trim() || accent2
    }
    syncVars()

    const resize = () => {
      const dpr = window.devicePixelRatio || 1
      const w = Math.max(1, Math.floor(canvas.clientWidth * dpr))
      const h = Math.max(1, Math.floor(canvas.clientHeight * dpr))
      if (canvas.width === w && canvas.height === h) return
      canvas.width = w
      canvas.height = h
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    }
    resize()
    const ro = new ResizeObserver(resize)
    ro.observe(canvas)

    // Apply the live blur without a full style object churn.
    const applyBlur = (px: number) => {
      canvas.style.filter = `blur(${px}px) saturate(1.5)`
    }
    applyBlur(paramsRef.current.blur)

    // Smoothed bar heights so the spectrum eases instead of flickering.
    const smoothed = new Float32Array(MAX_BARS)
    // Gentle idle pulse so the visualizer breathes even on silence/pause.
    let phase = 0

    const render = () => {
      const p = paramsRef.current
      if (canvas.style.filter !== `blur(${p.blur}px) saturate(1.5)`) applyBlur(p.blur)
      const w = canvas.clientWidth
      const h = canvas.clientHeight
      const dt = 1 / 60
      phase += dt * p.phaseSpeed
      const f = frameRef.current
      const energy = f ? f.level : 0
      const bins = f?.bins
      const barCount = bins && bins.length > 0 ? Math.min(bins.length, MAX_BARS) : MAX_BARS

      ctx.clearRect(0, 0, w, h)

      // Full-screen radial halo whose bloom tracks overall energy. Motion here
      // comes from `phase` (steady) and `energy` (audio), never from volume —
      // so the visualizer animates even when muted, and reacts to real audio.
      const beat = 0.5 + 0.5 * Math.sin(phase * 1.6)
      const a = p.bloomAlpha + beat * p.bloomBeat + energy * p.bloomEnergy
      const bloomR = Math.hypot(w, h) * (p.bloomRadius + beat * 0.12 + energy * 0.1)
      const bloom = ctx.createRadialGradient(w / 2, h * 0.52, 0, w / 2, h * 0.52, bloomR)
      bloom.addColorStop(0, hexToRgba(accent, a))
      bloom.addColorStop(0.45, hexToRgba(accent2, a * 0.72))
      bloom.addColorStop(1, 'transparent')
      ctx.fillStyle = bloom
      ctx.fillRect(0, 0, w, h)

      // Frequency bars from the real FFT bins. Each bar eases toward its target
      // magnitude so transients look lively but the field doesn't strobe. Bars
      // are tall (up to the full viewport height) and bold so the spectrum
      // reads as a large, prominent animation behind the album art.
      const gap = p.barGap
      const bw = (w - gap * (barCount - 1)) / barCount
      for (let i = 0; i < barCount; i++) {
        const target = bins && bins.length === barCount ? bins[i] : 0
        // Attack fast, release slow — mimics a real VU/spectrum meter.
        const k = target > smoothed[i] ? 0.5 : 0.12
        smoothed[i] += (target - smoothed[i]) * k
        const v = smoothed[i]
        const idle = p.barIdle * (0.5 + 0.5 * Math.sin(phase * 1.3 + i * 0.5))
        const amp = Math.max(idle, v)
        // Bars reach nearly the full height on strong bins.
        const bh = h * (p.barIdle + amp * p.barPeak)
        const x = i * (bw + gap)
        const y = h - bh
        const grad = ctx.createLinearGradient(0, y, 0, h)
        grad.addColorStop(0, accent2)
        grad.addColorStop(1, accent)
        ctx.fillStyle = grad
        const r = Math.min(bw / 2, p.barRadius)
        roundRect(ctx, x, y, bw, bh, r)
        ctx.fill()
      }
    }

    const loop = () => {
      render()
      rafRef.current = requestAnimationFrame(loop)
    }

    if (!reducedRef.current) {
      rafRef.current = requestAnimationFrame(loop)
    } else {
      render()
    }

    return () => {
      if (rafRef.current !== null) cancelAnimationFrame(rafRef.current)
      rafRef.current = null
      ro.disconnect()
    }
  }, [playing])

  return <canvas ref={canvasRef} className={styles.canvas} aria-hidden />
}

function hexToRgba(hex: string, alpha: number): string {
  const m = hex.replace('#', '')
  if (m.length < 6) return `rgba(110,231,183,${alpha})`
  const r = parseInt(m.slice(0, 2), 16)
  const g = parseInt(m.slice(2, 4), 16)
  const b = parseInt(m.slice(4, 6), 16)
  return `rgba(${r},${g},${b},${alpha})`
}

function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
) {
  const rr = Math.min(r, w / 2, h / 2)
  ctx.beginPath()
  ctx.moveTo(x + rr, y)
  ctx.arcTo(x + w, y, x + w, y + h, rr)
  ctx.arcTo(x + w, y + h, x, y + h, rr)
  ctx.arcTo(x, y + h, x, y, rr)
  ctx.arcTo(x, y, x + w, y, rr)
  ctx.closePath()
}
