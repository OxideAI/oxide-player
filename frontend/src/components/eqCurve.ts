import type { EqBand } from '../types'

/**
 * Biquad magnitude (in dB) for one EQ band at frequency `f`, given the
 * band's center freq/gain/Q and the sample rate `fs`. Formulas are the
 * canonical RBJ audio-EQ-cookbook transfer functions evaluated on the
 * unit circle; the result is 20*log10(|H(e^jw)|).
 *
 * Shelf filters use the standard RBJ form with the shelf slope derived
 * from Q, matching what CamillaDSP renders for the same band.
 */
function bandMagnitudeDb(band: EqBand, f: number, fs: number): number {
  const { type: band_type, freq: f0, gain: g, q } = band
  // Below 1Hz or above Nyquist the filter is meaningless for display.
  if (f0 <= 0 || f >= fs / 2 || f <= 0) return 0
  const w = (2 * Math.PI * f) / fs
  const w0 = (2 * Math.PI * f0) / fs
  const cosw = Math.cos(w)
  const sinw = Math.sin(w)
  const cosw0 = Math.cos(w0)
  const sinw0 = Math.sin(w0)
  // RBJ "A" gain factor. Peaking: A = 10^(g/20) so |H(w0)| = A = the
  // requested peak in dB. Shelves: A = 10^(g/40) so the asymptotic shelf
  // gain equals g dB. (RBJ's cookbook uses 10^(g/40) for both, but that
  // halves the peaking peak; we use the audibly-correct split.)
  const A = band_type === 'peaking' ? Math.pow(10, g / 20) : Math.pow(10, g / 40)
  const alpha = sinw0 / (2 * q)
  const sqA = Math.sqrt(A)

  // RBJ coefficients (a0-normalized). |H(e^jw)| = sqrt(re^2 + im^2) of the
  // complex response of the difference equation, then *20 log10.
  let b0: number, b1: number, b2: number
  let a0: number, a1: number, a2: number
  if (band_type === 'peaking') {
    b0 = 1 + alpha * A
    b1 = -2 * cosw0
    b2 = 1 - alpha * A
    a0 = 1 + alpha
    a1 = -2 * cosw0
    a2 = 1 - alpha
  } else if (band_type === 'low_shelf') {
    b0 = A * (A + 1 - (A - 1) * cosw0 + 2 * sqA * alpha)
    b1 = 2 * A * (A - 1 - (A + 1) * cosw0)
    b2 = A * (A + 1 - (A - 1) * cosw0 - 2 * sqA * alpha)
    a0 = A + 1 + (A - 1) * cosw0 + 2 * sqA * alpha
    a1 = -2 * (A - 1 + (A + 1) * cosw0)
    a2 = A + 1 + (A - 1) * cosw0 - 2 * sqA * alpha
  } else {
    // high_shelf
    b0 = A * (A + 1 + (A - 1) * cosw0 + 2 * sqA * alpha)
    b1 = -2 * A * (A - 1 + (A + 1) * cosw0)
    b2 = A * (A + 1 + (A - 1) * cosw0 - 2 * sqA * alpha)
    a0 = A + 1 - (A - 1) * cosw0 + 2 * sqA * alpha
    a1 = 2 * (A - 1 - (A + 1) * cosw0)
    a2 = A + 1 - (A - 1) * cosw0 - 2 * sqA * alpha
  }
  const numRe = b0 + b1 * cosw + b2 * Math.cos(2 * w)
  const numIm = -(b1 * sinw + b2 * Math.sin(2 * w))
  const denRe = a0 + a1 * cosw + a2 * Math.cos(2 * w)
  const denIm = -(a1 * sinw + a2 * Math.sin(2 * w))
  const Hre = (numRe * denRe + numIm * denIm) / (denRe * denRe + denIm * denIm)
  const Him = (numIm * denRe - numRe * denIm) / (denRe * denRe + denIm * denIm)
  const mag = Math.sqrt(Hre * Hre + Him * Him)
  return 20 * Math.log10(Math.max(mag, 1e-9))
}

export interface CurvePoint {
  /** Frequency in Hz */
  f: number
  /** Sum of band gains in dB at this frequency */
  db: number
}

export const FREQ_MIN = 20
export const FREQ_MAX = 20000
export const DB_MIN = -24
export const DB_MAX = 24
/**
 * Sample rate for the biquad computation only. The shape of the displayed
 * curve is dominated by the filter centers, all within the audible band,
 * so a constant 48 kHz is fine for display.
 */
const CURVE_FS = 48000

const N = 200

/**
 * Summed frequency response of an EQ band set across a log-spaced
 * [FREQ_MIN, FREQ_MAX] range, in dB. Bands must already be sorted by freq
 * (the editor does this on every draft change).
 */
export function eqResponse(bands: EqBand[]): CurvePoint[] {
  const logMin = Math.log(FREQ_MIN)
  const logMax = Math.log(FREQ_MAX)
  const pts: CurvePoint[] = []
  for (let i = 0; i < N; i++) {
    const f = Math.exp(logMin + (i / (N - 1)) * (logMax - logMin))
    let db = 0
    for (const b of bands) db += bandMagnitudeDb(b, f, CURVE_FS)
    pts.push({ f, db })
  }
  return pts
}

/** Axis: standard ISO frequencies. */
export const FREQ_TICKS = [20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000]
/** dB grid ticks. */
export const DB_TICKS = [-24, -18, -12, -6, 0, 6, 12, 18, 24]
/** Format a Hz value for the axis label. */
export function formatHz(hz: number): string {
  if (hz >= 1000) {
    const k = hz / 1000
    return Number.isInteger(k) ? `${k}k` : `${k}`
  }
  return String(hz)
}
