import { describe, expect, it } from 'vitest'
import {
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
