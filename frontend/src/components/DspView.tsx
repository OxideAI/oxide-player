import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  DeviceOutput,
  DspApplyResult,
  DspMode,
  DspProfile,
  DspSettings,
  EqBand,
  EqBandType,
  ResamplePreset,
} from "../types";
import { api } from "../api";
import { EqGraph } from "./EqGraph";
import styles from "./DspView.module.css";

const PRESETS: ResamplePreset[] = ["balanced", "high", "extreme"];
const BAND_TYPES: EqBandType[] = ["peaking", "low_shelf", "high_shelf"];
const SAMPLE_RATES = [44100, 48000, 88200, 96000, 176400, 192000];

/** Stable per-band identity used as React key when the band list is reordered. */
type BandRow = EqBand & { _id: number };

let _bandSeq = 1;
const nextBandId = () => _bandSeq++;

/** Draft profile that carries stable band row ids so live re-sorting never
 *  loses React element identity (and thus focus) mid-keystroke. */
type Draft = {
  device: string;
  mode: DspMode;
  target_rate: number | null;
  preset: ResamplePreset;
  preamp: number;
  eq_bands: BandRow[];
};

function defaultProfile(device: string): DspProfile {
  return {
    device,
    mode: "bit_perfect",
    target_rate: null,
    preset: "balanced",
    preamp: 0,
    eq_bands: [],
  };
}

function formatDspNumber(value: number, signed = false): string {
  const normalized = Object.is(value, -0) ? 0 : value;
  const formatted = normalized.toFixed(2);
  return signed && normalized >= 0 ? `+${formatted}` : formatted;
}

/** Serialize the import/export subset shared with AutoEQ-style text files. */
export function formatDspSettings(
  settings: Pick<DspProfile, "preamp" | "eq_bands">,
): string {
  const lines = [`Preamp: ${formatDspNumber(settings.preamp, true)} dB`, ""];
  const filterTypes: Record<EqBandType, string> = {
    peaking: "PK",
    low_shelf: "LS",
    high_shelf: "HS",
  };
  settings.eq_bands.forEach((band, index) => {
    lines.push(
      `Filter ${String(index + 1).padStart(2, " ")}: ON ${filterTypes[band.type]} Fc ${formatDspNumber(band.freq)} Hz Gain ${formatDspNumber(band.gain, true)} dB Q ${formatDspNumber(band.q)}`,
    );
  });
  return `${lines.join("\n")}\n`;
}
function ProfileEditor({
  profile,
  onSave,
  onRemove,
  advanced = true,
  onDirtyChange,
}: {
  profile: DspProfile;
  onSave: (p: DspProfile) => Promise<unknown>;
  onRemove?: () => void;
  advanced?: boolean;
  onDirtyChange?: (dirty: boolean) => void;
}) {
  // Draft carries stable _id'd band rows so re-sorting the band list by freq
  // never resets React element identity (and thus input focus).
  const toRows = useCallback(
    (p: DspProfile): Draft => ({
      device: p.device,
      mode: p.mode,
      target_rate: p.target_rate,
      preset: p.preset,
      preamp: p.preamp,
      eq_bands: p.eq_bands.map((b) => ({ ...b, _id: nextBandId() })),
    }),
    [],
  );
  const [draft, setDraft] = useState<Draft>(() => toRows(profile));
  const [saving, setSaving] = useState(false);
  const [importUrl, setImportUrl] = useState("");
  const [importing, setImporting] = useState(false);
  const [importNotice, setImportNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editingAdvanced, setEditingAdvanced] = useState(advanced);
  const previousProfile = useRef(profile);

  useEffect(() => {
    if (previousProfile.current === profile) return;
    previousProfile.current = profile;
    setDraft(toRows(profile));
    onDirtyChange?.(false);
  }, [profile, toRows, onDirtyChange]);

  // Sorted bands for display & graph. Live re-sort is deferred to the freq
  // input's blur so typing a frequency mid-edit never yanks the focused row
  // out from under the user; add/remove/type changes do an immediate sort.
  const sortedBands = useMemo(
    () => [...draft.eq_bands].sort((a, b) => a.freq - b.freq),
    [draft.eq_bands],
  );

  const update = (patch: Partial<Draft>) => {
    onDirtyChange?.(true);
    setDraft((d) => ({ ...d, ...patch }));
  };

  const updateBand = (id: number, patch: Partial<EqBand>) => {
    onDirtyChange?.(true);
    setDraft((d) => ({
      ...d,
      eq_bands: d.eq_bands.map((b) => (b._id === id ? { ...b, ...patch } : b)),
    }));
  };

  const sortBandsByFreq = () => {
    onDirtyChange?.(true);
    setDraft((d) => ({
      ...d,
      eq_bands: [...d.eq_bands].sort((a, b) => a.freq - b.freq),
    }));
  };

  const addBand = () => {
    onDirtyChange?.(true);
    setDraft((d) => ({
      ...d,
      eq_bands: [
        ...d.eq_bands,
        { type: "peaking", freq: 1000, gain: 0, q: 1, _id: nextBandId() },
      ],
    }));
  };

  const removeBand = (id: number) => {
    onDirtyChange?.(true);
    setDraft((d) => ({
      ...d,
      eq_bands: d.eq_bands.filter((b) => b._id !== id),
    }));
  };

  const applyImported = (settings: DspSettings) => {
    onDirtyChange?.(true);
    setDraft((d) => ({
      ...d,
      preamp: settings.preamp,
      eq_bands: settings.eq_bands.map((b) => ({ ...b, _id: nextBandId() })),
    }));
    setImportNotice(
      `Imported ${settings.eq_bands.length} filter${settings.eq_bands.length === 1 ? "" : "s"}. Click Apply to activate.`,
    );
  };

  const importText = async (text: string) => {
    setImporting(true);
    setError(null);
    setImportNotice(null);
    try {
      applyImported(await api.importDspText(text));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setImporting(false);
    }
  };

  const importFile = async (file: File) => {
    try {
      await importText(await file.text());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const importUrlSettings = async () => {
    if (!importUrl.trim()) {
      setError("A DSP settings URL is required.");
      return;
    }
    setImporting(true);
    setError(null);
    setImportNotice(null);
    try {
      applyImported(await api.importDspUrl(importUrl.trim()));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setImporting(false);
    }
  };

  const exportSettings = () => {
    const text = formatDspSettings({
      preamp: draft.preamp,
      eq_bands: sortedBands,
    });
    const blobUrl = URL.createObjectURL(
      new Blob([text], { type: "text/plain;charset=utf-8" }),
    );
    const link = document.createElement("a");
    const name =
      draft.device.trim().replace(/[^a-z0-9._-]+/gi, "-") || "dsp-settings";
    link.href = blobUrl;
    link.download = `${name}-eq.txt`;
    document.body.appendChild(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(blobUrl);
  };

  const save = async () => {
    if (!draft.device.trim()) {
      setError("Device name is required.");
      return;
    }
    // Canonical ascending-by-freq order on Apply; the backend re-sorts in
    // effective() as a second guarantee, so stored state is always sorted.
    const ordered: DspProfile = {
      device: draft.device,
      mode: draft.mode,
      target_rate: draft.target_rate,
      preset: draft.preset,
      preamp: draft.preamp,
      eq_bands: sortedBands.map((b) => ({
        type: b.type,
        freq: b.freq,
        gain: b.gain,
        q: b.q,
      })),
    };
    setSaving(true);
    setError(null);
    try {
      await onSave(ordered);
      onDirtyChange?.(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };
  const resampling = draft.mode === "resample";
  const editorOpen = advanced || editingAdvanced;

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
          {(["bit_perfect", "resample"] as const).map((m) => (
            <button
              key={m}
              className={draft.mode === m ? styles.modeActive : styles.mode}
              onClick={() => update({ mode: m })}
            >
              {m === "bit_perfect" ? "Bit-perfect" : "Resample + DSP"}
            </button>
          ))}
        </div>
        {onRemove && (
          <button
            className={styles.remove}
            onClick={onRemove}
            aria-label="remove profile"
          >
            Remove
          </button>
        )}
      </div>
      {!advanced && (
        <button
          className={styles.advancedToggle}
          aria-expanded={editorOpen}
          onClick={() => setEditingAdvanced((open) => !open)}
        >
          {editorOpen ? "Hide advanced editing" : "Edit EQ and resampling"}
        </button>
      )}
      {editorOpen && (
        <>
          <fieldset className={styles.fields} disabled={!resampling}>
            <label className={styles.field}>
              <span>Target sample rate</span>
              <select
                value={draft.target_rate ?? ""}
                onChange={(e) =>
                  update({
                    target_rate: e.target.value ? Number(e.target.value) : null,
                  })
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
                onChange={(e) =>
                  update({ preset: e.target.value as ResamplePreset })
                }
              >
                {PRESETS.map((p) => (
                  <option key={p} value={p}>
                    {p}
                  </option>
                ))}
              </select>
            </label>
          </fieldset>

          <div className={styles.preampRow}>
            <label className={styles.field}>
              <span>Preamp gain (dB)</span>
              <input
                aria-label="Preamp gain (dB)"
                type="number"
                min={-150}
                max={150}
                step={0.1}
                value={draft.preamp}
                onChange={(e) => update({ preamp: Number(e.target.value) })}
              />
            </label>
            <p className={styles.help}>
              Applied before the EQ filters to prevent imported boosts from
              clipping.
            </p>
          </div>

          <div className={styles.importBox}>
            <div className={styles.importTitle}>Import DSP settings</div>
            <div className={styles.importControls}>
              <label className={styles.fileButton}>
                <span>Choose file</span>
                <input
                  aria-label="DSP settings file"
                  className={styles.fileInput}
                  type="file"
                  accept=".txt,text/plain"
                  disabled={importing}
                  onChange={(e) => {
                    const file = e.currentTarget.files?.[0];
                    if (file) void importFile(file);
                    e.currentTarget.value = "";
                  }}
                />
              </label>
              <span className={styles.importOr}>or</span>
              <input
                className={styles.importUrl}
                aria-label="DSP settings URL"
                type="url"
                placeholder="https://…/equalizer.txt"
                value={importUrl}
                disabled={importing}
                onChange={(e) => setImportUrl(e.target.value)}
              />
              <button
                className={styles.importButton}
                onClick={() => void importUrlSettings()}
                disabled={importing || !importUrl.trim()}
              >
                {importing ? "Importing…" : "Import URL"}
              </button>
            </div>
            <p className={styles.help}>
              Only Preamp and numbered Filter values are imported. Apply saves
              them to this profile.
            </p>
            {importNotice && (
              <div className={styles.notice}>{importNotice}</div>
            )}
          </div>

          <div className={styles.eqHead}>
            <span>Equalizer</span>
            <button className={styles.add} onClick={addBand}>
              + Add band
            </button>
          </div>

          {/* Summed EQ frequency response (20Hz–20kHz, ±24 dB), log axis.
          Bands are sorted ascending by freq, matching the persisted order. */}
          <EqGraph bands={sortedBands} />

          <div className={styles.bands}>
            {sortedBands.length === 0 && (
              <p className={styles.dim}>
                No EQ bands. Add one to shape the tone (works in both modes).
              </p>
            )}
            {sortedBands.map((b) => (
              <div key={b._id} className={styles.band}>
                <select
                  value={b.type}
                  onChange={(e) =>
                    updateBand(b._id, { type: e.target.value as EqBandType })
                  }
                >
                  {BAND_TYPES.map((t) => (
                    <option key={t} value={t}>
                      {t.replace("_", " ")}
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
                    onChange={(e) =>
                      updateBand(b._id, { freq: Number(e.target.value) })
                    }
                    onBlur={sortBandsByFreq}
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
                    onChange={(e) =>
                      updateBand(b._id, { gain: Number(e.target.value) })
                    }
                  />
                </label>
                <label className={styles.bandField}>
                  Q
                  <input
                    type="number"
                    step={0.1}
                    min={0.1}
                    value={b.q}
                    onChange={(e) =>
                      updateBand(b._id, { q: Number(e.target.value) })
                    }
                  />
                </label>
                <button
                  className={styles.remove}
                  onClick={() => removeBand(b._id)}
                  aria-label="remove band"
                >
                  ✕
                </button>
              </div>
            ))}
          </div>
        </>
      )}
      {error && <div className={styles.error}>{error}</div>}
      <div className={styles.actions}>
        <button className={styles.export} onClick={exportSettings}>
          Export .txt
        </button>
        <button className={styles.save} onClick={save} disabled={saving}>
          {saving ? "Applying…" : "Apply"}
        </button>
      </div>
    </div>
  );
}

export interface DspViewProps {
  selectedOutput?: DeviceOutput | null;
  outputs?: DeviceOutput[];
  onSelectOutput?: (selectionKey: string | null) => void;
  onRefresh?: () => Promise<void> | void;
}

export function DspView({
  selectedOutput = undefined,
  outputs: _outputs,
  onSelectOutput,
  onRefresh,
}: DspViewProps = {}) {
  const [profiles, setProfiles] = useState<DspProfile[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);
  const [applyState, setApplyState] = useState<DspApplyResult | null>(null);
  const [pendingOutputKey, setPendingOutputKey] = useState<string | null>(null);
  const previousOutputKey = useRef<string | null>(
    selectedOutput?.selection_key ?? null,
  );

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const got = await api.dsp();
      setProfiles(
        got.length
          ? got
          : [defaultProfile(selectedOutput?.dsp_device ?? "default")],
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [selectedOutput?.dsp_device]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    const nextKey = selectedOutput?.selection_key ?? null;
    if (nextKey === previousOutputKey.current) return;
    if (dirty && previousOutputKey.current && onSelectOutput) {
      setPendingOutputKey(nextKey);
      onSelectOutput(previousOutputKey.current);
      return;
    }
    previousOutputKey.current = nextKey;
    setApplyState(null);
  }, [dirty, onSelectOutput, selectedOutput?.selection_key]);

  const upsert = useCallback((p: DspProfile) => {
    setProfiles((list) => {
      const i = list.findIndex((x) => x.device === p.device);
      if (i >= 0) return list.map((x, j) => (j === i ? p : x));
      return [...list, p];
    });
  }, []);

  const remove = useCallback((device: string) => {
    setProfiles((list) => list.filter((x) => x.device !== device));
  }, []);

  const addProfile = useCallback(() => {
    setProfiles((list) => [
      ...list,
      defaultProfile(`device-${list.length + 1}`),
    ]);
  }, []);

  const selectedProfile = selectedOutput?.dsp_device
    ? profiles.find((profile) => profile.device === selectedOutput.dsp_device)
    : undefined;
  const editorProfiles = selectedOutput
    ? [
        selectedProfile ??
          defaultProfile(selectedOutput.dsp_device ?? selectedOutput.name),
      ]
    : profiles;
  const blockedReason = selectedOutput?.diagnostic_code;
  const dspUnavailable = Boolean(
    selectedOutput &&
    (!selectedOutput.dsp_supported ||
      blockedReason === "unsupported_output_type" ||
      blockedReason === "disconnected" ||
      blockedReason === "inactive"),
  );

  const apply = async (np: DspProfile) => {
    const result = (await api.setDsp(np)) as DspApplyResult | undefined;
    const normalized: DspApplyResult = result ?? {
      device: np.device,
      persisted: true,
      reload_confirmed: true,
      active: true,
    };
    setApplyState(normalized);
    upsert(np);
    if (normalized.persisted) await onRefresh?.();
  };

  if (loading) return <p className={styles.dim}>loading…</p>;
  if (error) return <div className={styles.error}>{error}</div>;

  return (
    <div className={styles.wrap}>
      <div>
        <span className={styles.eyebrow}>Engine</span>
        <h2 className={styles.h}>DSP profiles</h2>
      </div>
      {selectedOutput && (
        <section className={styles.routeStatus} aria-live="polite">
          <strong>{selectedOutput.name}</strong>
          {dspUnavailable ? (
            <p>
              {blockedReason === "disconnected"
                ? "DSP is unavailable while this Bluetooth output is disconnected."
                : blockedReason === "inactive"
                  ? "Enable this output before configuring its DSP route."
                  : "This output does not support DSP."}
            </p>
          ) : (
            <>
              <p>
                {selectedOutput.dsp_enabled
                  ? `DSP route active${selectedProfile ? ` · ${selectedProfile.device}` : ""}.`
                  : blockedReason === "missing_profile"
                    ? "DSP is supported, but this output has no saved profile yet."
                    : "DSP is supported for this output; the route is not active."}
              </p>
              {applyState && (
                <p
                  className={
                    applyState.reload_confirmed
                      ? styles.success
                      : styles.warning
                  }
                >
                  {applyState.reload_confirmed
                    ? "Saved and reload-confirmed active."
                    : `Saved, but the DSP route is not confirmed.${applyState.reload_error ? ` ${applyState.reload_error}` : ""}`}
                </p>
              )}
            </>
          )}
        </section>
      )}
      {!selectedOutput && (
        <p className={styles.dim}>
          Bit-perfect bypasses the resampler for unchanged passthrough. Select a
          playback output to verify DSP routing before editing advanced
          settings.
        </p>
      )}
      {selectedOutput && dspUnavailable ? (
        <p className={styles.dim}>
          Choose a supported, active playback output to edit DSP settings.
        </p>
      ) : (
        <>
          {editorProfiles.map((p, i) => (
            <ProfileEditor
              key={p.device || i}
              profile={p}
              advanced={!selectedOutput}
              onDirtyChange={setDirty}
              onSave={apply}
              onRemove={
                !selectedOutput && profiles.length > 1
                  ? () => remove(p.device)
                  : undefined
              }
            />
          ))}
          {!selectedOutput && (
            <button className={styles.addProfile} onClick={addProfile}>
              + Add DSP profile
            </button>
          )}
        </>
      )}
      {pendingOutputKey !== null && (
        <div
          className={styles.switchDialog}
          role="dialog"
          aria-modal="true"
          aria-labelledby="dsp-switch-title"
        >
          <h3 id="dsp-switch-title">Unsaved DSP edits</h3>
          <p>
            Keep editing this profile, or discard the draft before switching
            outputs?
          </p>
          <div className={styles.switchActions}>
            <button onClick={() => setPendingOutputKey(null)}>
              Keep editing
            </button>
            <button
              onClick={() => {
                setDirty(false);
                previousOutputKey.current = pendingOutputKey;
                onSelectOutput?.(pendingOutputKey);
                setPendingOutputKey(null);
              }}
            >
              Discard and switch
            </button>
            <button onClick={() => setPendingOutputKey(null)}>Cancel</button>
          </div>
        </div>
      )}
    </div>
  );
}
