import { useMemo } from 'react'
import type { EqBand } from '../types'
import {
  eqResponse,
  FREQ_MIN,
  FREQ_MAX,
  DB_MIN,
  DB_MAX,
  FREQ_TICKS,
  DB_TICKS,
  formatHz,
} from './eqCurve'
import styles from './EqGraph.module.css'

const W = 560
const H = 160
const PADDING = { top: 10, right: 12, bottom: 22, left: 30 }

const plotW = W - PADDING.left - PADDING.right
const plotH = H - PADDING.top - PADDING.bottom

const freqX = (f: number) =>
  PADDING.left + (Math.log(f) - Math.log(FREQ_MIN)) / (Math.log(FREQ_MAX) - Math.log(FREQ_MIN)) * plotW
const dbY = (db: number) =>
  PADDING.top + (1 - (db - DB_MIN) / (DB_MAX - DB_MIN)) * plotH

/**
 * A read-only curve view of the summed EQ response for a band set.
 * Vertical axis spans ±24 dB, log frequency axis from 20Hz to 20kHz.
 * Bands must already be sorted by freq; band centers are marked with
 * dotted vertical guides so the user can compare centers to the curve.
 */
export function EqGraph({ bands }: { bands: EqBand[] }) {
  const { areaPath, linePath } = useMemo(() => {
    const pts = eqResponse(bands)
    let line = ''
    pts.forEach((p, i) => {
      const x = freqX(p.f)
      const y = dbY(p.db)
      line += `${i === 0 ? 'M' : 'L'}${x.toFixed(2)},${y.toFixed(2)} `
    })
    const baselineY = dbY(0)
    const lastX = freqX(pts[pts.length - 1].f)
    const firstX = freqX(pts[0].f)
    const area = `${line}L${lastX.toFixed(2)},${baselineY.toFixed(2)} L${firstX.toFixed(2)},${baselineY.toFixed(2)} Z`
    return { areaPath: area, linePath: line }
  }, [bands])

  return (
    <svg
      className={styles.graph}
      viewBox={`0 0 ${W} ${H}`}
      preserveAspectRatio="xMidYMid meet"
      role="img"
      aria-label="Equalizer frequency response"
    >
      {/* dB grid + labels */}
      {DB_TICKS.map((db) => (
        <g key={`db-${db}`}>
          <line
            x1={PADDING.left}
            x2={W - PADDING.right}
            y1={dbY(db)}
            y2={dbY(db)}
            className={db === 0 ? styles.zeroLine : styles.gridLine}
          />
          <text x={PADDING.left - 4} y={dbY(db) + 3} textAnchor="end" className={styles.tickLbl}>
            {db > 0 ? `+${db}` : db}
          </text>
        </g>
      ))}
      {/* freq grid + labels */}
      {FREQ_TICKS.map((f) => (
        <g key={`f-${f}`}>
          <line
            x1={freqX(f)}
            x2={freqX(f)}
            y1={PADDING.top}
            y2={H - PADDING.bottom}
            className={styles.gridLine}
          />
          <text x={freqX(f)} y={H - PADDING.bottom + 14} textAnchor="middle" className={styles.tickLbl}>
            {formatHz(f)}
          </text>
        </g>
      ))}
      {/* band-center guides */}
      {bands.map((b, i) => (
        <line
          key={`band-${i}-${b.freq}`}
          x1={freqX(Math.max(FREQ_MIN, Math.min(FREQ_MAX, b.freq)))}
          x2={freqX(Math.max(FREQ_MIN, Math.min(FREQ_MAX, b.freq)))}
          y1={PADDING.top}
          y2={H - PADDING.bottom}
          className={styles.bandGuide}
        />
      ))}
      {/* area fill under the curve */}
      <path d={areaPath} className={styles.area} />
      {/* response curve */}
      <path d={linePath} className={styles.curve} fill="none" />
    </svg>
  )
}