import { useState } from "react";
import type { DeviceOutput, PlayerStatus } from "../types";
import styles from "./AudioPathView.module.css";

export const STATUS_STALE_AFTER_MS = 15_000;

export type AudioPathState =
  | "healthy"
  | "paused"
  | "stopped"
  | "mpd-error"
  | "stale"
  | "unavailable"
  | "zero-output"
  | "multiple-output";

export interface AudioPathReductionInput {
  status: PlayerStatus | null;
  outputs: DeviceOutput[];
  connected: boolean;
  connectionError?: string | null;
  lastSnapshotAt?: number | null;
  now?: number;
  detailsReady?: boolean;
}

export interface AudioPathReduction {
  state: AudioPathState;
  enabledOutputs: DeviceOutput[];
  eligibleOutputs: DeviceOutput[];
  message: string;
}

function liveOutput(
  output: DeviceOutput,
  status: PlayerStatus | null,
): boolean {
  const live = status?.outputs.find(
    (candidate) => candidate.id === output.id || candidate.name === output.name,
  );
  return live ? live.enabled : output.enabled;
}

export function stableOutputIdentity(
  output: Pick<DeviceOutput, "selection_key" | "name">,
): string {
  return output.selection_key || `managed:${output.name}`;
}

export function reduceAudioPath({
  status,
  outputs,
  connected,
  connectionError,
  lastSnapshotAt,
  now = Date.now(),
  detailsReady = true,
}: AudioPathReductionInput): AudioPathReduction {
  const eligibleOutputs = outputs.filter(
    (output) => output.role === "playback" && output.selectable,
  );
  const runtimeOutputs = (status?.outputs ?? [])
    .filter(
      (output) => !["visualizer", "camilladsp-loopback"].includes(output.name),
    )
    .map((output) => ({
      ...output,
      role: "playback" as const,
      selectable: true,
      selection_key: `runtime:${output.name}`,
      configured: false,
      available: true,
      connected: null,
      active: output.enabled,
      dsp_supported: output.dsp_supported === true,
      dsp_enabled: output.dsp_enabled === true,
    }));
  const enabledOutputs = (
    detailsReady ? eligibleOutputs : runtimeOutputs
  ).filter((output) => liveOutput(output, status));

  if (!status) {
    return {
      state: "unavailable",
      enabledOutputs,
      eligibleOutputs,
      message:
        connectionError ??
        "The player status is unavailable. Check the Oxide service and retry.",
    };
  }
  if (
    !connected ||
    (lastSnapshotAt !== null &&
      lastSnapshotAt !== undefined &&
      now - lastSnapshotAt > STATUS_STALE_AFTER_MS)
  ) {
    return {
      state: "stale",
      enabledOutputs,
      eligibleOutputs,
      message:
        "Playback status is out of date. The last known route is shown until the player reconnects.",
    };
  }
  if (status.error) {
    return {
      state: "mpd-error",
      enabledOutputs,
      eligibleOutputs,
      message: `The player reported an error: ${status.error}`,
    };
  }
  if (!detailsReady && status.outputs.length === 0) {
    return {
      state: "zero-output",
      enabledOutputs,
      eligibleOutputs,
      message: "No playback output is currently reported by the player.",
    };
  }
  if (detailsReady && eligibleOutputs.length === 0) {
    return {
      state: "zero-output",
      enabledOutputs,
      eligibleOutputs,
      message:
        "No listening output is configured. Choose an output to send library playback to.",
    };
  }
  if (enabledOutputs.length > 1) {
    return {
      state: "multiple-output",
      enabledOutputs,
      eligibleOutputs,
      message:
        "Multiple playback outputs are enabled. Choose one route or resolve the extras.",
    };
  }
  if (status.state === "playing") {
    return {
      state: "healthy",
      enabledOutputs,
      eligibleOutputs,
      message: "Audio is playing through the selected output.",
    };
  }
  if (status.state === "paused") {
    return {
      state: "paused",
      enabledOutputs,
      eligibleOutputs,
      message: "The route is ready, but playback is paused.",
    };
  }
  return {
    state: "stopped",
    enabledOutputs,
    eligibleOutputs,
    message: "The route is configured, but playback is stopped.",
  };
}

const stateLabels: Record<AudioPathState, string> = {
  healthy: "Healthy",
  paused: "Paused",
  stopped: "Stopped",
  "mpd-error": "MPD error",
  stale: "Status stale",
  unavailable: "Player unavailable",
  "zero-output": "No output",
  "multiple-output": "Multiple outputs",
};

const diagnosticLabels: Record<string, string> = {
  reload_error:
    "Output changes are waiting for MPD or DSP reload confirmation.",
  unsupported_output_type: "This output does not support DSP.",
  disconnected: "The configured output is disconnected.",
  inactive: "The output is configured but currently inactive.",
  missing_profile:
    "DSP is supported, but no profile is applied to this output.",
  unknown_output: "This output is visible for technical inspection only.",
};

export interface AudioPathViewProps extends AudioPathReductionInput {
  selectedOutputKey: string | null;
  onSelectOutput: (selectionKey: string) => void;
  onToggleOutput?: (output: DeviceOutput) => Promise<void> | void;
  onToggleDsp?: (output: DeviceOutput) => Promise<void> | void;
  onRetry?: () => void;
  onResolveMultiple?: () => Promise<void> | void;
  deviceError?: string | null;
  actionError?: string | null;
}

export function AudioPathView({
  selectedOutputKey,
  onSelectOutput,
  onToggleOutput,
  onToggleDsp,
  onRetry,
  onResolveMultiple,
  deviceError,
  actionError,
  ...reductionInput
}: AudioPathViewProps) {
  const reduction = reduceAudioPath(reductionInput);
  const selected =
    reduction.eligibleOutputs.find(
      (output) => stableOutputIdentity(output) === selectedOutputKey,
    ) ?? null;
  const [confirmResolve, setConfirmResolve] = useState(false);
  const [busy, setBusy] = useState(false);

  const invokeResolve = async () => {
    if (!onResolveMultiple) return;
    setBusy(true);
    try {
      await onResolveMultiple();
      setConfirmResolve(false);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className={styles.wrap}>
      <section
        className={styles.routeCard}
        aria-labelledby="audio-path-heading"
      >
        <div className={styles.routeHead}>
          <div>
            <span className={styles.eyebrow}>Playback path</span>
            <h2 className={styles.title} id="audio-path-heading">
              Library → output
            </h2>
          </div>
          <span
            className={`${styles.state} ${styles[`state_${reduction.state}`]}`}
          >
            {stateLabels[reduction.state]}
          </span>
        </div>
        <p className={styles.message} role="status" aria-live="polite">
          {reduction.message}
        </p>
        {reduction.enabledOutputs.length === 1 ? (
          <p className={styles.destination}>
            Listening destination:{" "}
            <strong>{reduction.enabledOutputs[0].name}</strong>
          </p>
        ) : reduction.enabledOutputs.length > 1 ? (
          <p className={styles.destination}>
            Listening destination is unresolved until one output remains
            enabled.
          </p>
        ) : (
          <p className={styles.destination}>No active listening destination.</p>
        )}
        {reduction.state === "multiple-output" && onResolveMultiple && (
          <button
            className={styles.primary}
            onClick={() => setConfirmResolve(true)}
          >
            Resolve extra outputs
          </button>
        )}
        {reduction.state === "unavailable" && onRetry && (
          <button className={styles.secondary} onClick={onRetry}>
            Retry player status
          </button>
        )}
        {deviceError && (
          <p className={styles.warning}>
            Output details could not be refreshed. Route health is based on the
            live player snapshot.
          </p>
        )}
        {actionError && (
          <p className={styles.error} role="alert">
            {actionError}
          </p>
        )}
      </section>

      <section className={styles.outputs} aria-labelledby="outputs-heading">
        <div className={styles.sectionHead}>
          <div>
            <span className={styles.eyebrow}>Choose a destination</span>
            <h3 className={styles.sectionTitle} id="outputs-heading">
              Playback outputs
            </h3>
          </div>
          <span className={styles.count}>
            {reduction.eligibleOutputs.length}
          </span>
        </div>
        {reduction.eligibleOutputs.length === 0 ? (
          <p className={styles.muted}>
            No configured listening outputs are available. System outputs such
            as the visualizer stay separate.
          </p>
        ) : (
          <div className={styles.outputList}>
            {reduction.eligibleOutputs.map((output) => {
              const key = stableOutputIdentity(output);
              const selectedClass =
                key === selectedOutputKey ? styles.outputSelected : "";
              return (
                <button
                  type="button"
                  key={key}
                  className={`${styles.outputRow} ${selectedClass}`}
                  aria-pressed={key === selectedOutputKey}
                  onClick={() => onSelectOutput(key)}
                >
                  <span>
                    <strong>{output.name}</strong>
                    <span className={styles.outputMeta}>
                      {output.connected === false
                        ? "Disconnected"
                        : output.connected === true
                          ? "Connected"
                          : output.available
                            ? "Available"
                            : "Unavailable"}
                      {" · "}
                      {output.enabled ? "Enabled" : "Disabled"}
                    </span>
                  </span>
                  <span className={styles.outputAction}>
                    {key === selectedOutputKey ? "Selected" : "Inspect"}
                  </span>
                </button>
              );
            })}
          </div>
        )}
      </section>

      {selected && (
        <section
          className={styles.controls}
          aria-labelledby="selected-output-heading"
        >
          <div className={styles.sectionHead}>
            <div>
              <span className={styles.eyebrow}>Selected output</span>
              <h3 className={styles.sectionTitle} id="selected-output-heading">
                {selected.name}
              </h3>
            </div>
            <span className={styles.outputMeta}>
              {selected.dsp_supported ? "DSP supported" : "DSP unavailable"}
            </span>
          </div>
          {selected.diagnostic_code && (
            <p className={styles.warning}>
              {diagnosticLabels[selected.diagnostic_code] ??
                selected.diagnostic_code}
            </p>
          )}
          <div className={styles.controlRow}>
            {onToggleOutput && (
              <button
                className={styles.secondary}
                onClick={() => void onToggleOutput(selected)}
              >
                {selected.enabled ? "Disable output" : "Enable output"}
              </button>
            )}
            {onToggleDsp && selected.dsp_supported && (
              <button
                className={styles.secondary}
                onClick={() => void onToggleDsp(selected)}
              >
                {selected.dsp_enabled ? "Disable DSP" : "Enable DSP"}
              </button>
            )}
          </div>
          <details className={styles.technical}>
            <summary>Technical details</summary>
            <dl>
              <div>
                <dt>Selection identity</dt>
                <dd>{stableOutputIdentity(selected)}</dd>
              </div>
              <div>
                <dt>Runtime output</dt>
                <dd>{selected.id}</dd>
              </div>
              {selected.technical_detail && (
                <div>
                  <dt>Backend detail</dt>
                  <dd>{selected.technical_detail}</dd>
                </div>
              )}
            </dl>
          </details>
        </section>
      )}

      {confirmResolve && (
        <div
          className={styles.dialogOverlay}
          role="presentation"
          onClick={() => setConfirmResolve(false)}
        >
          <div
            className={styles.dialog}
            role="dialog"
            aria-modal="true"
            aria-labelledby="resolve-heading"
            onClick={(event) => event.stopPropagation()}
          >
            <h3 id="resolve-heading">Disable extra outputs?</h3>
            <p>
              This keeps {reduction.enabledOutputs[0]?.name ?? "one output"}{" "}
              enabled and disables the other active routes.
            </p>
            <div className={styles.controlRow}>
              <button
                className={styles.secondary}
                onClick={() => setConfirmResolve(false)}
                disabled={busy}
              >
                Cancel
              </button>
              <button
                className={styles.primary}
                onClick={() => void invokeResolve()}
                disabled={busy}
              >
                {busy ? "Resolving…" : "Disable extras"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
