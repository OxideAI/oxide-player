import { describe, expect, it } from 'vitest'
import {
  DEFAULT_VIZ_PARAMS,
  drawBars,
  drawMirrored,
  getSpectrumTargets,
  hasSpectrumSignal,
  REFERENCE_VIZ_STYLES,
  type VizStyle,
} from './Visualizer'

const frame = {
  bins: [0, 0.05, 0.2, 0.8, 0.35, 0.1, 0.02],
  level: 0.6,
}

describe('visualizer audio mapping', () => {
  it('activates when the capture stream has signal even if playback status is stopped', () => {
    expect(hasSpectrumSignal(null)).toBe(false)
    expect(hasSpectrumSignal({ bins: [0, 0.01], level: 0 })).toBe(true)
  })
  it('keeps a real spectrum signal visible in each reference style', () => {
    for (const style of REFERENCE_VIZ_STYLES.map(({ id }) => id) as VizStyle[]) {
      const targets = getSpectrumTargets(style, frame, 16)
      expect(Math.max(...targets), `${style} flattened the audio signal`).toBeGreaterThan(0.1)
      expect(targets).toHaveLength(16)
    }
  })

  it('resamples backend bins instead of dropping audio when the bar count changes', () => {
    const targets = getSpectrumTargets('bars', { bins: [0.2, 0.9, 0.4], level: 0.4 }, 16)
    expect(targets.some((value) => value > 0.5)).toBe(true)
  })
})

describe('visualizer bar drawing', () => {
  // The smoothed buffer is MAX_BARS (256) entries while the active spectrum
  // is only `count` (72) bins. The draw loop must honor `count` — drawing
  // the whole buffer renders 184 extra idle stubs and squeezes the signal
  // into the left third of the screen.
  function recordingCtx() {
    const calls: Array<{ op: string; args: number[] }> = []
    const ctx = {
      beginPath: () => calls.push({ op: 'beginPath', args: [] }),
      closePath: () => calls.push({ op: 'closePath', args: [] }),
      moveTo: (x: number, y: number) => calls.push({ op: 'moveTo', args: [x, y] }),
      arcTo: (x1: number, y1: number, x2: number, y2: number) => calls.push({ op: 'arcTo', args: [x1, y1, x2, y2] }),
      fill: () => calls.push({ op: 'fill', args: [] }),
      createLinearGradient: () => ({ addColorStop: () => {} }),
      createRadialGradient: () => ({ addColorStop: () => {} }),
      set fillStyle(_v: unknown) {},
      set strokeStyle(_v: unknown) {},
    } as unknown as CanvasRenderingContext2D
    return { ctx, calls }
  }

  it('draws exactly `count` bars, not every smoothed buffer entry', () => {
    const { ctx, calls } = recordingCtx()
    const values = new Float32Array(256)
    values.fill(0.5, 0, 72)
    values[100] = 1 // must not create a bar when count = 72
    drawBars(ctx, 1920, 1080, values, 72, DEFAULT_VIZ_PARAMS, '#6ee7b7', '#8b9cff', 0)
    const fills = calls.filter((c) => c.op === 'fill')
    expect(fills).toHaveLength(72)
  })

  it('spreads the bars across the full width (bar width from `count`)', () => {
    const { ctx, calls } = recordingCtx()
    const values = new Float32Array(256)
    values.fill(0.5, 0, 72)
    drawBars(ctx, 1920, 1080, values, 72, DEFAULT_VIZ_PARAMS, '#6ee7b7', '#8b9cff', 0)
    // bar 0's first arcTo starts at its right edge (x + barWidth).
    const firstRight = calls.find((c) => c.op === 'arcTo')!.args[0]
    const expected = (1920 - DEFAULT_VIZ_PARAMS.barGap * 71) / 72
    expect(firstRight).toBeCloseTo(expected, 0)
  })

  it('mirrored style draws two fills per bar and honors `count`', () => {
    const { ctx, calls } = recordingCtx()
    const values = new Float32Array(256)
    values.fill(0.5, 0, 72)
    drawMirrored(ctx, 1920, 1080, values, 72, DEFAULT_VIZ_PARAMS, '#6ee7b7', '#8b9cff')
    const fills = calls.filter((c) => c.op === 'fill')
    expect(fills).toHaveLength(144)
  })
})
