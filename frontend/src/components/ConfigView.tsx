import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../api";
import type { Config, DeviceOutput, PlayerStatus } from "../types";
import { AudioPathView, stableOutputIdentity } from "./AudioPathView";
import { DevicesView } from "./DevicesView";
import { DspView } from "./DspView";
import { VisualizationSettings } from "./VisualizationSettings";
import styles from "./ConfigView.module.css";

type Section =
  | "library"
  | "mpd"
  | "server"
  | "bluetooth"
  | "dsp"
  | "storage"
  | "visualizer";

export interface ConfigViewProps {
  status?: PlayerStatus | null;
  statusConnected?: boolean;
  statusError?: string | null;
  statusLastUpdatedAt?: number | null;
}

export function ConfigView({
  status = null,
  statusConnected = false,
  statusError = null,
  statusLastUpdatedAt = null,
}: ConfigViewProps) {
  const [config, setConfig] = useState<Config | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [restartNeeded, setRestartNeeded] = useState(false);
  const [version, setVersion] = useState<{ version: string } | null>(null);
  const [deviceOutputs, setDeviceOutputs] = useState<DeviceOutput[]>([]);
  const [deviceLoading, setDeviceLoading] = useState(true);
  const [deviceError, setDeviceError] = useState<string | null>(null);
  const [selectedOutputKey, setSelectedOutputKey] = useState<string | null>(
    null,
  );
  const [actionError, setActionError] = useState<string | null>(null);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [newDir, setNewDir] = useState("");
  const [scanning, setScanning] = useState(false);
  const [saving, setSaving] = useState<Section | null>(null);
  const [shuttingDown, setShuttingDown] = useState(false);
  const [restarting, setRestarting] = useState(false);
  const [confirmAction, setConfirmAction] = useState<
    "restart" | "shutdown" | null
  >(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setConfig(await api.getConfig());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);
  const loadDevices = useCallback(async () => {
    setDeviceLoading(true);
    setDeviceError(null);
    try {
      setDeviceOutputs(await api.devices());
    } catch (e) {
      setDeviceError(e instanceof Error ? e.message : String(e));
    } finally {
      setDeviceLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadDevices();
  }, [loadDevices]);

  const eligibleOutputs = useMemo(
    () =>
      deviceOutputs.filter(
        (output) => output.role === "playback" && output.selectable,
      ),
    [deviceOutputs],
  );
  const selectedOutput = useMemo(
    () =>
      eligibleOutputs.find(
        (output) => stableOutputIdentity(output) === selectedOutputKey,
      ) ?? null,
    [eligibleOutputs, selectedOutputKey],
  );

  useEffect(() => {
    if (!selectedOutputKey) {
      const active = eligibleOutputs.find((output) => output.enabled);
      if (active) setSelectedOutputKey(stableOutputIdentity(active));
      return;
    }
    if (
      !eligibleOutputs.some(
        (output) => stableOutputIdentity(output) === selectedOutputKey,
      )
    ) {
      setSelectedOutputKey(null);
    }
  }, [eligibleOutputs, selectedOutputKey]);

  useEffect(() => {
    api
      .version()
      .then(setVersion)
      .catch(() => setVersion(null));
  }, []);

  const patch = (p: Partial<Config>) =>
    setConfig((c) => (c ? { ...c, ...p } : c));
  const save = async (
    which: Section,
    restart: boolean,
    overrideConfig?: Config,
  ): Promise<Config | null> => {
    const nextConfig = overrideConfig ?? config;
    if (!nextConfig) return null;
    setSaving(which);
    setError(null);
    setNotice(null);
    try {
      const saved = await api.updateConfig(nextConfig);
      setConfig(saved);
      if (restart) setRestartNeeded(true);
      setNotice("Saved.");
      return saved;
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      return null;
    } finally {
      setSaving(null);
    }
  };

  const toggleOutput = async (output: DeviceOutput) => {
    setActionError(null);
    try {
      if (output.enabled) await api.disableDevice(output.id);
      else await api.enableDevice(output.id);
      await loadDevices();
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  };

  const toggleDsp = async (output: DeviceOutput) => {
    setActionError(null);
    try {
      if (output.dsp_enabled) await api.disableDeviceDsp(output.id);
      else await api.enableDeviceDsp(output.id);
      await loadDevices();
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  };

  const resolveMultiple = async () => {
    const active = eligibleOutputs.filter((output) => output.enabled);
    if (active.length < 2) return;
    setActionError(null);
    try {
      for (const output of active.slice(1)) await api.disableDevice(output.id);
      await loadDevices();
      setNotice(
        "Extra playback outputs disabled. Waiting for live player status to confirm one route.",
      );
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  };
  const addDir = async () => {
    const path = newDir.trim();
    if (!path) return;
    setError(null);
    setNotice(null);
    try {
      const res = await api.addLibraryDir(path);
      setNewDir("");
      await load();
      setNotice(
        res.duplicate
          ? "Folder is already a source."
          : `Added shared folder and rescanned ${res.scanned} track(s).`,
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const removeDir = async (path: string) => {
    setError(null);
    setNotice(null);
    try {
      await api.removeLibraryDir(path);
      await load();
      setNotice("Removed source folder and share.");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const rescan = async () => {
    setScanning(true);
    setError(null);
    setNotice(null);
    try {
      const res = await api.refresh();
      setNotice(`Rescanned ${res.scanned} track(s).`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setScanning(false);
    }
  };

  const shutdown = async () => {
    setShuttingDown(true);
    setConfirmAction(null);
    setError(null);
    setNotice(null);
    try {
      await api.shutdown();
      // The server is going away; there is no response to wait for.
      setNotice("Server is shutting down.");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setShuttingDown(false);
    }
  };

  const restart = async () => {
    setRestarting(true);
    setConfirmAction(null);
    setError(null);
    setNotice(null);
    try {
      await api.restart();
      setNotice("Server is restarting.");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setRestarting(false);
    }
  };

  return (
    <div className={styles.wrap}>
      <div>
        <span className={styles.eyebrow}>Server</span>
        <h2 className={styles.h}>Settings</h2>
      </div>

      {restartNeeded && (
        <div className={styles.restart}>
          Some changes only take effect after a server restart. The file on disk
          is already updated.
        </div>
      )}
      {error && <div className={styles.error}>{error}</div>}
      {notice && <div className={styles.notice}>{notice}</div>}
      <AudioPathView
        status={status}
        outputs={deviceOutputs}
        connected={statusConnected}
        connectionError={statusError}
        lastSnapshotAt={statusLastUpdatedAt}
        detailsReady={!deviceLoading}
        selectedOutputKey={selectedOutputKey}
        onSelectOutput={setSelectedOutputKey}
        onToggleOutput={toggleOutput}
        onToggleDsp={toggleDsp}
        onRetry={() => void loadDevices()}
        onResolveMultiple={resolveMultiple}
        deviceError={deviceError}
        actionError={actionError}
      />
      {config && (
        <VisualizationSettings
          config={config}
          onSave={(enabled) =>
            save("visualizer", true, { ...config, visualizer_fft: enabled })
          }
        />
      )}
      {!config && loading && (
        <p className={styles.dim}>Loading administration settings…</p>
      )}
      {config && (
        <details
          className={styles.admin}
          open={advancedOpen}
          onToggle={(event) => setAdvancedOpen(event.currentTarget.open)}
        >
          <summary className={styles.adminSummary}>
            Administration and advanced controls
          </summary>

          <section className={styles.card}>
            <div className={styles.cardHead}>
              <h3 className={styles.cardTitle}>Music library sources</h3>
              <span className={styles.live}>applies immediately</span>
            </div>
            <ul className={styles.dirList}>
              {config!.library_dirs.map((d) => (
                <li key={d} className={styles.dirRow}>
                  <span className={styles.dirPath}>{d}</span>
                  <button
                    className={styles.iconBtnDanger}
                    onClick={() => removeDir(d)}
                    aria-label="Remove source"
                  >
                    Remove
                  </button>
                </li>
              ))}
              {config!.library_dirs.length === 0 && (
                <li className={styles.dim}>No library sources configured.</li>
              )}
            </ul>
            <div className={styles.saveRow}>
              <input
                className={styles.input}
                placeholder="/absolute/path/to/music"
                value={newDir}
                onChange={(e) => setNewDir(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && addDir()}
              />
              <button
                className={styles.save}
                onClick={addDir}
                disabled={!newDir.trim()}
              >
                Add folder
              </button>
              <button
                className={styles.iconBtn}
                onClick={rescan}
                disabled={scanning}
              >
                {scanning ? "Rescanning..." : "Rescan library"}
              </button>
            </div>
            <p className={styles.hint}>
              Folders must be under MPD's music_directory to be playable. After
              adding, a rescan syncs MPD's index; if a track still won't play,
              check the folder is inside MPD's music_directory.
            </p>
          </section>

          <section className={styles.card}>
            <div className={styles.cardHead}>
              <h3 className={styles.cardTitle}>Playback (MPD)</h3>
              <span className={styles.restartTag}>restart required</span>
            </div>
            <div className={styles.grid}>
              <label className={styles.field}>
                <span>MPD host</span>
                <input
                  className={styles.input}
                  value={config!.mpd_host}
                  onChange={(e) => patch({ mpd_host: e.target.value })}
                />
              </label>
              <label className={styles.field}>
                <span>MPD port</span>
                <input
                  className={styles.input}
                  type="number"
                  value={config!.mpd_port}
                  onChange={(e) => patch({ mpd_port: Number(e.target.value) })}
                />
              </label>
            </div>
            <button
              className={styles.save}
              onClick={() => save("mpd", true)}
              disabled={saving === "mpd"}
            >
              {saving === "mpd" ? "Saving..." : "Save MPD settings"}
            </button>
          </section>

          <section className={styles.card}>
            <div className={styles.cardHead}>
              <h3 className={styles.cardTitle}>Server</h3>
              <span className={styles.restartTag}>restart required</span>
            </div>
            <label className={styles.field}>
              <span>Listen address</span>
              <input
                className={styles.input}
                value={config!.listen}
                onChange={(e) => patch({ listen: e.target.value })}
              />
            </label>
            <p className={styles.hint}>
              Binding to a non-loopback address exposes the (currently
              unauthenticated) API beyond this machine.
            </p>
            <button
              className={styles.save}
              onClick={() => save("server", true)}
              disabled={saving === "server"}
            >
              {saving === "server" ? "Saving..." : "Save server settings"}
            </button>
          </section>

          <section className={styles.card}>
            <div className={styles.cardHead}>
              <h3 className={styles.cardTitle}>Bluetooth</h3>
              <span className={styles.restartTag}>restart required</span>
            </div>
            <label className={styles.checkField}>
              <input
                className={styles.checkbox}
                type="checkbox"
                checked={config!.bluetooth_reconnect_on_startup}
                onChange={(e) =>
                  patch({ bluetooth_reconnect_on_startup: e.target.checked })
                }
              />
              <span>Reconnect paired speakers on startup</span>
            </label>
            <p className={styles.hint}>
              On launch, Oxide reconnects paired speakers with managed BlueALSA
              outputs. Disable this if another service owns Bluetooth connection
              policy.
            </p>
            <button
              className={styles.save}
              onClick={() => save("bluetooth", true)}
              disabled={saving === "bluetooth"}
            >
              {saving === "bluetooth" ? "Saving..." : "Save Bluetooth settings"}
            </button>
          </section>

          <section className={styles.card}>
            <div className={styles.cardHead}>
              <h3 className={styles.cardTitle}>DSP (CamillaDSP)</h3>
              <span className={styles.restartTag}>restart required</span>
            </div>
            <div className={styles.grid}>
              <label className={styles.field}>
                <span>Config path</span>
                <input
                  className={styles.input}
                  value={config!.camilladsp_config_path}
                  onChange={(e) =>
                    patch({ camilladsp_config_path: e.target.value })
                  }
                />
              </label>
              <label className={styles.field}>
                <span>WebSocket URL</span>
                <input
                  className={styles.input}
                  value={config!.camilladsp_ws_url ?? ""}
                  placeholder="ws://127.0.0.1:1234"
                  onChange={(e) =>
                    patch({
                      camilladsp_ws_url: e.target.value ? e.target.value : null,
                    })
                  }
                />
              </label>
            </div>
            <button
              className={styles.save}
              onClick={() => save("dsp", true)}
              disabled={saving === "dsp"}
            >
              {saving === "dsp" ? "Saving..." : "Save DSP settings"}
            </button>
          </section>

          <section className={styles.card}>
            <div className={styles.cardHead}>
              <h3 className={styles.cardTitle}>Storage</h3>
              <span className={styles.restartTag}>restart required</span>
            </div>
            <div className={styles.grid}>
              <label className={styles.field}>
                <span>Data directory</span>
                <input
                  className={styles.input}
                  value={config!.data_dir}
                  readOnly
                />
              </label>
              <label className={styles.field}>
                <span>Static (UI) directory</span>
                <input
                  className={styles.input}
                  value={config!.static_dir}
                  readOnly
                />
              </label>
            </div>
            <p className={styles.hint}>
              These are set at startup. Edit the config file and restart to
              change them.
            </p>
            <p className={styles.dim}>
              Fields flagged restart required above are written to the config
              file now and applied on next launch.
            </p>
          </section>

          <section className={styles.card}>
            <div className={styles.cardHead}>
              <h3 className={styles.cardTitle}>Playback devices</h3>
              <span className={styles.live}>live</span>
            </div>
            {advancedOpen && (
              <DevicesView
                outputs={deviceOutputs}
                selectedOutputKey={selectedOutputKey}
                onSelectOutput={setSelectedOutputKey}
                onRefresh={loadDevices}
              />
            )}
          </section>

          <section className={styles.card}>
            <div className={styles.cardHead}>
              <h3 className={styles.cardTitle}>DSP engine</h3>
              <span className={styles.live}>live</span>
            </div>
            {advancedOpen && (
              <DspView
                selectedOutput={selectedOutput}
                outputs={deviceOutputs}
                onSelectOutput={setSelectedOutputKey}
                onRefresh={loadDevices}
              />
            )}
          </section>

          <section className={styles.card}>
            <div className={styles.cardHead}>
              <h3 className={styles.cardTitle}>Server power</h3>
              <span className={styles.restartTag}>stops playback</span>
            </div>
            <p className={styles.hint}>
              Power off or reboot the machine running oxide-player. Reboot
              requires a supervising systemd unit to bring the server back up;
              power off leaves the machine off until started again.
            </p>
            <div className={styles.saveRow}>
              <button
                className={styles.save}
                onClick={() => setConfirmAction("restart")}
                disabled={restarting || shuttingDown}
              >
                {restarting ? "Rebooting..." : "Reboot server"}
              </button>
              <button
                className={styles.danger}
                onClick={() => setConfirmAction("shutdown")}
                disabled={shuttingDown || restarting}
              >
                {shuttingDown ? "Powering off..." : "Power off server"}
              </button>
            </div>
          </section>
        </details>
      )}
      {confirmAction && (
        <div
          className={styles.confirmOverlay}
          onClick={() => setConfirmAction(null)}
        >
          <div
            className={styles.confirmBox}
            onClick={(e) => e.stopPropagation()}
          >
            <p className={styles.confirmText}>
              {confirmAction === "restart"
                ? "Reboot the machine running the Oxide server? Playback will pause and the machine will restart."
                : "Power off the machine running the Oxide server? This stops playback and the machine will shut down."}
            </p>
            <div className={styles.confirmActions}>
              <button
                className={styles.iconBtn}
                onClick={() => setConfirmAction(null)}
              >
                Cancel
              </button>
              <button
                className={styles.danger}
                onClick={confirmAction === "restart" ? restart : shutdown}
              >
                {confirmAction === "restart" ? "Reboot" : "Power off"}
              </button>
            </div>
          </div>
        </div>
      )}

      {version && <p className={styles.dim}>Version {version.version}</p>}
    </div>
  );
}
