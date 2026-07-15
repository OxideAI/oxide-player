import { useCallback, useEffect, useState } from 'react'
import type { DspProfile, EqBand, EqBandType, ResamplePreset } from '../types'
import { api } from '../api'
import styles from './DspView.module.css'

const PRESETS: ResamplePreset[] = ['balanced', 'high', 'extreme']
const BAND_TYPES: EqBandType[] = ['peaking', 'low_shelf', 'high_shelf']
const SAMPLE_RATES = [44100, 48000, 88200, 96000, 176400, 192000]

function clone(p: DspProfile): DspProfile {
  return { ...p, eq_bands: p.eq_bands.map((b) => ({ ...b })) }
}

function defaultProfile(device: string): DspProfile {
  return { device, mode: 'bit_perfect', target_rate: null, preset: 'balanced', eq_bands: [] }
}

function ProfileEditor({
  profile,
  onSave,
  onRemove,
}: {
  profile: DspProfile
  onSave: (p: DspProfile) => Promise<unknown>
  onRemove?: () => void
}) {
  const [draft, setDraft] = useState<DspProfile>(() => clone(profile))
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => setDraft(clone(profile)), [profile])

  const update = (patch: Partial<DspProfile>) => setDraft((d) => ({ ...d, ...patch }))

  const updateBand = (i: number, patch: Partial<EqBand>) =>
    setDraft((d) => ({
      ...d,
      eq_bands: d.eq_bands.map((b, j) => (j === i ? { ...b, ...patch } : b)),
    }))

  const addBand = () =>
    setDraft((d) => ({
      ...d,
      eq_bands: [...d.eq_bands, { type: 'peaking', freq: 1000, gain: 0, q: 1 }],
    }))

  const removeBand = (i: number) =>
    setDraft((d) => ({ ...d, eq_bands: d.eq_bands.filter((_, j) => j !== i) }))

  const save = async () => {
    if (!draft.device.trim()) {
      setError('Device name is required.')
      return
    }
    setSaving(true)
    setError(null)
    try {
      await onSave(draft)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setSaving(false)
    }
  }

  const resampling = draft.mode === 'resample'

  return (
    <div className={styles.profile}>
      <div className={styles.head}>
        <label className={styles.deviceField}>
          <span className={styles.deviceLabel}>Device</span>
          <input
            className={styles.deviceInput}
            value={draft.device}
            placeholder="CamillaDSP playback device"
            onChange={(e) => update({ device: e.target.value })}
          />
        </label>
        <div className={styles.modes}>
          {(['bit_perfect', 'resample'] as const).map((m) => (
            <button
              key={m}
              className={draft.mode === m ? styles.modeActive : styles.mode}
              onClick={() => update({ mode: m })}
            >
              {m === 'bit_perfect' ? 'Bit-perfect' : 'Resample + DSP'}
            </button>
          ))}
        </div>
        {onRemove && (
          <button className={styles.remove} onClick={onRemove} aria-label="remove profile">
            Remove
          </button>
        )}
      </div>

      <fieldset className={styles.fields} disabled={!resampling}>
        <label className={styles.field}>
          <span>Target sample rate</span>
          <select
            value={draft.target_rate ?? ''}
            onChange={(e) =>
              update({ target_rate: e.target.value ? Number(e.target.value) : null })
            }
          >
            <option value="">— pick a rate —</option>
            {SAMPLE_RATES.map((r) => (
              <option key={r} value={r}>
                {r}
              </option>
            ))}
          </select>
        </label>

        <label className={styles.field}>
          <span>Resample quality</span>
          <select
            value={draft.preset}
            onChange={(e) => update({ preset: e.target.value as ResamplePreset })}
          >
            {PRESETS.map((p) => (
              <option key={p} value={p}>
                {p}
              </option>
            ))}
          </select>
        </label>
      </fieldset>

      <div className={styles.eqHead}>
        <span>Equalizer</span>
        <button className={styles.add} onClick={addBand}>
          + Add band
        </button>
      </div>

      <div className={styles.bands}>
        {draft.eq_bands.length === 0 && (
          <p className={styles.dim}>No EQ bands. Add one to shape the tone (works in both modes).</p>
        )}
        {draft.eq_bands.map((b, i) => (
          <div key={i} className={styles.band}>
            <select
              value={b.type}
              onChange={(e) => updateBand(i, { type: e.target.value as EqBandType })}
            >
              {BAND_TYPES.map((t) => (
                <option key={t} value={t}>
                  {t.replace('_', ' ')}
                </option>
              ))}
            </select>
            <label className={styles.bandField}>
              freq
              <input
                type="number"
                min={10}
                max={24000}
                step={1}
                value={b.freq}
                onChange={(e) => updateBand(i, { freq: Number(e.target.value) })}
              />
            </label>
            <label className={styles.bandField}>
              gain
              <input
                type="number"
                min={-20}
                max={20}
                step={0.5}
                value={b.gain}
                onChange={(e) => updateBand(i, { gain: Number(e.target.value) })}
              />
            </label>
            <label className={styles.bandField}>
              Q
              <input
                type="number"
                step={0.1}
                min={0.1}
                value={b.q}
                onChange={(e) => updateBand(i, { q: Number(e.target.value) })}
              />
            </label>
            <button
              className={styles.remove}
              onClick={() => removeBand(i)}
              aria-label="remove band"
            >
              ✕
            </button>
          </div>
        ))}
      </div>

      {error && <div className={styles.error}>{error}</div>}
      <button className={styles.save} onClick={save} disabled={saving}>
        {saving ? 'Applying…' : 'Apply'}
      </button>
    </div>
  )
}

export function DspView() {
  const [profiles, setProfiles] = useState<DspProfile[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const got = await api.dsp()
      // Always surface at least one editable profile so the EQ is reachable
      // even when no DSP profiles are configured server-side.
      setProfiles(got.length ? got : [defaultProfile('default')])
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    load()
  }, [load])

  const upsert = useCallback((p: DspProfile) => {
    setProfiles((list) => {
      const i = list.findIndex((x) => x.device === p.device)
      if (i >= 0) return list.map((x, j) => (j === i ? p : x))
      return [...list, p]
    })
  }, [])

  const remove = useCallback((device: string) => {
    setProfiles((list) => list.filter((x) => x.device !== device))
  }, [])

  const addProfile = useCallback(() => {
    setProfiles((list) => [...list, defaultProfile(`device-${list.length + 1}`)])
  }, [])

  if (loading) return <p className={styles.dim}>loading…</p>
  if (error) return <div className={styles.error}>{error}</div>

  return (
    <div className={styles.wrap}>
      <div>
        <span className={styles.eyebrow}>Engine</span>
        <h2 className={styles.h}>DSP profiles</h2>
      </div>
      <p className={styles.dim}>
        Bit-perfect bypasses the resampler for unchanged passthrough. Resample + DSP changes the
        output sample rate via a Soxr resampler. The parametric EQ below can be used in either mode
        (R10: bit-perfect applies EQ without resampling). Set the device to your CamillaDSP playback
        device name.
      </p>

      {profiles.map((p, i) => (
        <ProfileEditor
          key={i}
          profile={p}
          onSave={async (np) => {
            await api.setDsp(np)
            upsert(np)
          }}
          onRemove={profiles.length > 1 ? () => remove(p.device) : undefined}
        />
      ))}

      <button className={styles.addProfile} onClick={addProfile}>
        + Add DSP profile
      </button>
    </div>
  )
}
