import { useState } from 'react'
import {
  REFERENCE_VIZ_STYLES,
  VIZ_PRESETS,
  type VizParams,
  type VizStyle,
} from './Visualizer'
import { api } from '../api'
import styles from './VisualizerControls.module.css'

type NumericVizKey = Exclude<keyof VizParams, 'style'>

interface SliderDef {
  key: NumericVizKey
  label: string
  min: number
  max: number
  step: number
}

const SLIDERS: SliderDef[] = [
  { key: 'bloomAlpha', label: 'Halo opacity', min: 0, max: 1, step: 0.01 },
  { key: 'bloomBeat', label: 'Halo beat', min: 0, max: 0.6, step: 0.01 },
  { key: 'bloomEnergy', label: 'Halo energy', min: 0, max: 0.8, step: 0.01 },
  { key: 'bloomRadius', label: 'Halo size', min: 0.4, max: 2, step: 0.01 },
  { key: 'barIdle', label: 'Bar min', min: 0, max: 0.5, step: 0.01 },
  { key: 'barPeak', label: 'Bar max', min: 0.2, max: 1, step: 0.01 },
  { key: 'barGap', label: 'Bar gap', min: 0, max: 10, step: 1 },
  { key: 'barRadius', label: 'Bar radius', min: 0, max: 12, step: 1 },
  { key: 'phaseSpeed', label: 'Pulse speed', min: 0.2, max: 4, step: 0.1 },
  { key: 'blur', label: 'Blur', min: 0, max: 20, step: 1 },
]

interface Props {
  params: VizParams
  onChange: (next: VizParams) => void
  onClose: () => void
}

/**
 * Tuning panel for the Kiosk visualizer, reachable from the "Tune visualizer"
 * button in Kiosk mode. Lets you drag the look live and Save the resulting
 * `VizParams` to disk on the server (`/api/visualizer/params`, written to
 * `<data_dir>/vizparams.json`) so they persist across restarts. The JSON
 * snippet is shown for reference / manual editing.
 */
export function VisualizerControls({ params, onChange, onClose }: Props) {
  const [saved, setSaved] = useState(false)
  const snippet = JSON.stringify(params, null, 2)
  const set = (key: keyof VizParams, value: number) => {
    onChange({ ...params, [key]: value })
  }
  const save = () => {
    // Backend stores snake_case keys; map from the frontend camelCase shape.
    const body: Record<string, number | string> = {
      style: params.style,
      bloom_alpha: params.bloomAlpha,
      bloom_beat: params.bloomBeat,
      bloom_energy: params.bloomEnergy,
      bloom_radius: params.bloomRadius,
      bar_idle: params.barIdle,
      bar_peak: params.barPeak,
      bar_gap: params.barGap,
      bar_radius: params.barRadius,
      phase_speed: params.phaseSpeed,
      blur: params.blur,
    }
    api.saveVizParams(body)
      .then(() => {
        setSaved(true)
        setTimeout(() => setSaved(false), 1500)
      })
      .catch(() => {
        setSaved(false)
      })
  }

  return (
    <div className={styles.panel}>
      <div className={styles.head}>
        <span>Visualizer tuning</span>
        <button className={styles.close} onClick={onClose} aria-label="Close tuning">
          ×
        </button>
      </div>
      <label className={styles.stylePicker}>
        <span className={styles.label}>Visualization style</span>
        <select
          aria-label="Visualization style"
          value={params.style}
          onChange={(e) => onChange(VIZ_PRESETS[e.target.value as VizStyle])}
        >
          {REFERENCE_VIZ_STYLES.map((option) => (
            <option key={option.id} value={option.id}>
              {option.icon} {option.label}
            </option>
          ))}
        </select>
      </label>

      <div className={styles.sliders}>
        {SLIDERS.map((s) => (
          <label key={s.key} className={styles.row}>
            <span className={styles.label}>{s.label}</span>
            <input
              type="range"
              min={s.min}
              max={s.max}
              step={s.step}
              value={params[s.key]}
              onChange={(e) => set(s.key, Number(e.target.value))}
            />
            <span className={styles.val}>{params[s.key]}</span>
          </label>
        ))}
      </div>

      <button className={styles.copy} onClick={save}>
        {saved ? 'Saved!' : 'Save params'}
      </button>
      <textarea className={styles.snippet} readOnly value={snippet} rows={8} />
    </div>
  )
}
