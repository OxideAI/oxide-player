import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { Config, VisualizerStatus, VisualizerStatusState } from "../types";
import styles from "./VisualizationSettings.module.css";

export interface VisualizationSettingsProps {
  config: Config;
  onSave: (enabled: boolean) => Promise<Config | null>;
}

const statusCopy: Record<VisualizerStatusState, string> = {
  disabled: "Disabled",
  "enabled-pending-restart": "Enabled · restart pending",
  running: "Running",
  "waiting-for-capture": "Waiting for capture",
  "startup/runtime-error": "Capture error",
};

export function VisualizationSettings({
  config,
  onSave,
}: VisualizationSettingsProps) {
  const configuredEnabled = Boolean(config.visualizer_fft);
  const baseline = useRef(configuredEnabled);
  const [draftEnabled, setDraftEnabled] = useState(configuredEnabled);
  const [dirty, setDirty] = useState(false);
  const [conflict, setConflict] = useState(false);
  const [status, setStatus] = useState<VisualizerStatus | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  const refreshStatus = () => {
    const request = api.visualizerStatus?.();
    if (!request) return;
    void request.then(setStatus).catch((error) => {
      setStatusError(error instanceof Error ? error.message : String(error));
    });
  };

  useEffect(() => {
    refreshStatus();
  }, [config.visualizer_fft]);

  useEffect(() => {
    if (configuredEnabled === baseline.current) return;
    if (dirty) {
      setConflict(true);
      return;
    }
    baseline.current = configuredEnabled;
    setDraftEnabled(configuredEnabled);
  }, [configuredEnabled, dirty]);

  const onToggle = (enabled: boolean) => {
    setDraftEnabled(enabled);
    setDirty(enabled !== configuredEnabled);
    setConflict(false);
    setSaveError(null);
  };

  const cancel = () => {
    baseline.current = configuredEnabled;
    setDraftEnabled(configuredEnabled);
    setDirty(false);
    setConflict(false);
    setSaveError(null);
  };

  const save = async () => {
    setSaving(true);
    setSaveError(null);
    try {
      const saved = await onSave(draftEnabled);
      if (!saved) return;
      baseline.current = Boolean(saved.visualizer_fft);
      setDraftEnabled(Boolean(saved.visualizer_fft));
      setDirty(false);
      setConflict(false);
      refreshStatus();
    } catch (error) {
      setSaveError(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  const state =
    status?.status ??
    (configuredEnabled ? "enabled-pending-restart" : "disabled");
  const source =
    status?.configured_source ??
    config.visualizer_capture_device ??
    config.camilladsp_capture_device;
  const rate =
    status?.configured_rate ??
    config.visualizer_capture_rate ??
    config.camilladsp_capture_rate;

  return (
    <section
      className={styles.card}
      aria-labelledby="visualization-settings-title"
    >
      <div className={styles.head}>
        <div>
          <span className={styles.eyebrow}>Separate feature</span>
          <h3 id="visualization-settings-title" className={styles.title}>
            Visualization
          </h3>
        </div>
        <span className={styles.status} data-testid="visualizer-status">
          {statusCopy[state]}
        </span>
      </div>
      <label className={styles.toggle}>
        <input
          type="checkbox"
          checked={draftEnabled}
          onChange={(event) => onToggle(event.target.checked)}
          disabled={saving}
        />
        <span>Enable visualization</span>
      </label>
      <p className={styles.copy}>
        Visualization reads a separate capture path for the kiosk display. It is
        not a playback output.
      </p>
      <dl className={styles.details}>
        <div>
          <dt>Configured</dt>
          <dd>{configuredEnabled ? "Enabled" : "Disabled"}</dd>
        </div>
        <div>
          <dt>Applied process</dt>
          <dd>{status?.applied_enabled ? "Enabled" : "Disabled"}</dd>
        </div>
        <div>
          <dt>Capture source</dt>
          <dd>{source ?? "Default capture source"}</dd>
        </div>
        <div>
          <dt>Capture rate</dt>
          <dd>{rate ? `${rate} Hz` : "Device default"}</dd>
        </div>
        <div>
          <dt>Scope</dt>
          <dd>Oxide process restart required</dd>
        </div>
      </dl>
      {state === "waiting-for-capture" && (
        <p className={styles.info}>
          Capture is waiting for the FIFO or device. This is non-terminal and
          will retry.
        </p>
      )}
      {state === "startup/runtime-error" && (
        <p className={styles.error}>
          {status?.detail ?? "Capture could not start."}
        </p>
      )}
      {statusError && (
        <p className={styles.error}>Status unavailable: {statusError}</p>
      )}
      {conflict && (
        <p className={styles.error}>
          The saved visualization setting changed elsewhere. Cancel to use the
          refreshed value, or save this draft intentionally.
        </p>
      )}
      {saveError && <p className={styles.error}>Save failed: {saveError}</p>}
      <div className={styles.actions}>
        <button
          className={styles.save}
          onClick={() => void save()}
          disabled={!dirty || saving}
        >
          {saving ? "Saving…" : "Save visualization"}
        </button>
        <button
          className={styles.cancel}
          onClick={cancel}
          disabled={!dirty || saving}
        >
          Cancel
        </button>
      </div>
    </section>
  );
}
