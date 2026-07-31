import { useCallback, useEffect, useState } from 'react'
import type { BtDevice, DeviceConfig, InputStatusResponse, OutputDevice } from '../types'
import { api } from '../api'
import styles from './DevicesView.module.css'

interface FormData {
  name: string
  output_type: string
  device: string
  format: string
  mixer_type: string
  mixer_device: string
  dop: boolean
}

const emptyForm: FormData = {
  name: '',
  output_type: 'alsa',
  device: '',
  format: '',
  mixer_type: '',
  mixer_device: '',
  dop: false,
}

export function DevicesView() {
  // Runtime devices
  const [devices, setDevices] = useState<OutputDevice[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState<number | null>(null)

  // Managed configs
  const [configs, setConfigs] = useState<DeviceConfig[]>([])
  const [restartPending, setRestartPending] = useState(false)
  const [includeWarning, setIncludeWarning] = useState(false)
  const [restarting, setRestarting] = useState(false)

  // Add/edit form
  const [showForm, setShowForm] = useState(false)
  const [editing, setEditing] = useState<string | null>(null)
  const [form, setForm] = useState<FormData>(emptyForm)
  const [formError, setFormError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)

  // Confirmation dialog
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null)

  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const [d, c] = await Promise.all([api.devices(), api.deviceConfigs()])
      setDevices(d)
      setConfigs(c)
      // Derive restart_pending: true if any config has it set, false otherwise
      const anyPending = c.some((cfg) => cfg.restart_pending)
      setRestartPending(anyPending)
      setIncludeWarning(c.some((cfg) => cfg.include_warning))
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    load()
  }, [load])

  const toggle = async (d: OutputDevice) => {
    setBusy(d.id)
    setError(null)
    try {
      if (d.enabled) await api.disableDevice(d.id)
      else await api.enableDevice(d.id)
      await load()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(null)
    }
  }

  // Convenience: populate form data from a DeviceConfig
  const populateForm = (cfg: DeviceConfig) => {
    setForm({
      name: cfg.name,
      output_type: cfg.output_type,
      device: cfg.device ?? '',
      format: cfg.format ?? '',
      mixer_type: cfg.mixer_type ?? '',
      mixer_device: cfg.mixer_device ?? '',
      dop: cfg.dop,
    })
  }

  const openAddForm = () => {
    setEditing(null)
    setForm(emptyForm)
    setFormError(null)
    setShowForm(true)
  }

  const openEditForm = (cfg: DeviceConfig) => {
    setEditing(cfg.name)
    populateForm(cfg)
    setFormError(null)
    setShowForm(true)
  }

  const closeForm = () => {
    setShowForm(false)
    setEditing(null)
    setForm(emptyForm)
    setFormError(null)
  }

  const handleFormChange = (field: keyof FormData, value: string | boolean) => {
    setForm((prev) => ({ ...prev, [field]: value }))
  }

  const handleSave = async () => {
    setFormError(null)
    setSaving(true)
    try {
      if (editing) {
        await api.updateDeviceConfig(editing, form)
      } else {
        await api.createDeviceConfig(form)
      }
      setRestartPending(true)
      closeForm()
      await load()
    } catch (e) {
      setFormError(e instanceof Error ? e.message : String(e))
    } finally {
      setSaving(false)
    }
  }

  const handleDelete = async (name: string) => {
    setConfirmDelete(null)
    try {
      await api.deleteDeviceConfig(name)
      setRestartPending(true)
      await load()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }

  const handleRestart = async () => {
    setRestarting(true)
    setError(null)
    try {
      await api.restartMpd()
      setRestartPending(false)
      await load()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setRestarting(false)
    }
  }

  // Get a label for the output type
  const typeLabel = (t: string) => {
    const known: Record<string, string> = {
      alsa: 'ALSA',
      pulse: 'PulseAudio',
      fifo: 'FIFO',
      httpd: 'HTTP daemon',
      shout: 'Icecast',
      recorder: 'Recorder',
      null: 'Null',
      pipe: 'Pipe',
      jack: 'JACK',
      opensl: 'OpenSL ES',
      osx: 'CoreAudio',
      wasapi: 'WASAPI',
      winmm: 'Windows MM',
    }
    return known[t] ?? t
  }

  return (
    <div className={styles.wrap}>
      <div>
        <span className={styles.eyebrow}>System</span>
        <h2 className={styles.h}>Output devices</h2>
      </div>

      {error && <div className={styles.error}>{error}</div>}

      {/* Restart pending banner */}
      {restartPending && (
        <div className={styles.banner}>
          <span>Device configs changed — restart MPD to apply them.</span>
          <button
            className={styles.bannerBtn}
            disabled={restarting}
            onClick={handleRestart}
          >
            {restarting ? 'Restarting…' : 'Restart MPD'}
          </button>
        </div>
      )}

      {/* Include warning */}
      {includeWarning && (
        <div className={styles.error}>
          MPD config path not set. Add an <code>include</code> directive manually
          to your MPD config to load the managed output fragments.
        </div>
      )}

      {/* Runtime devices */}
      <div>
        <span className={styles.eyebrow}>Runtime</span>
        <h3 className={styles.h}>Active devices</h3>
      </div>
      {loading && <p className={styles.dim}>loading…</p>}
      {!loading && !error && devices.length === 0 && (
        <p className={styles.dim}>No output devices reported by MPD.</p>
      )}
      <ul className={styles.list}>
        {devices.map((d) => (
          <li key={d.id} className={d.enabled ? styles.rowOn : styles.rowOff}>
            <div>
              <div className={styles.name}>{d.name}</div>
              <div className={styles.id}>#{d.id}</div>
            </div>
            <button
              className={d.enabled ? styles.on : styles.off}
              disabled={busy === d.id}
              onClick={() => toggle(d)}
            >
              {d.enabled ? 'Enabled' : 'Disabled'}
            </button>
          </li>
        ))}
      </ul>

      {/* Managed configs */}
      <div className={styles.cfgSection}>
        <div>
          <span className={styles.eyebrow}>Configuration</span>
          <h3 className={styles.h}>Configured devices</h3>
        </div>

        {!showForm && (
          <div className={styles.cfgToolbar}>
            <button className={styles.addBtn} onClick={openAddForm}>
              + Add device
            </button>
          </div>
        )}

        {/* Add/Edit form */}
        {showForm && (
          <div className={styles.form}>
            {formError && <div className={styles.error}>{formError}</div>}
            <div className={styles.formRow}>
              <label className={styles.formLabel}>Name</label>
              <input
                className={styles.formInput}
                value={form.name}
                onChange={(e) => handleFormChange('name', e.target.value)}
                placeholder="My USB DAC"
              />
            </div>
            <div className={styles.formRow}>
              <label className={styles.formLabel}>Type</label>
              <input
                className={styles.formInput}
                value={form.output_type}
                onChange={(e) => handleFormChange('output_type', e.target.value)}
                placeholder="alsa, pulse, fifo, …"
                list="output-types"
              />
              <datalist id="output-types">
                <option value="alsa" />
                <option value="pulse" />
                <option value="fifo" />
                <option value="httpd" />
                <option value="shout" />
                <option value="recorder" />
                <option value="null" />
                <option value="pipe" />
                <option value="jack" />
                <option value="opensl" />
                <option value="osx" />
                <option value="wasapi" />
                <option value="winmm" />
              </datalist>
            </div>
            <div className={styles.formRow}>
              <label className={styles.formLabel}>Device</label>
              <input
                className={styles.formInput}
                value={form.device}
                onChange={(e) => handleFormChange('device', e.target.value)}
                placeholder="hw:0,0"
              />
            </div>
            <div className={styles.formRow}>
              <label className={styles.formLabel}>Format</label>
              <input
                className={styles.formInput}
                value={form.format}
                onChange={(e) => handleFormChange('format', e.target.value)}
                placeholder="44100:16:2"
              />
            </div>
            <div className={styles.formRow}>
              <label className={styles.formLabel}>Mixer type</label>
              <input
                className={styles.formInput}
                value={form.mixer_type}
                onChange={(e) => handleFormChange('mixer_type', e.target.value)}
                placeholder="hardware, software, none"
              />
            </div>
            <div className={styles.formRow}>
              <label className={styles.formLabel}>Mixer device</label>
              <input
                className={styles.formInput}
                value={form.mixer_device}
                onChange={(e) => handleFormChange('mixer_device', e.target.value)}
                placeholder="Master"
              />
            </div>
            <div className={styles.formCheck}>
              <input
                type="checkbox"
                id="dop-toggle"
                checked={form.dop}
                onChange={(e) => handleFormChange('dop', e.target.checked)}
              />
              <label htmlFor="dop-toggle" className={styles.formLabel}>
                DoP (DSD over PCM)
              </label>
            </div>
            <div className={styles.formActions}>
              <button
                className={styles.btnPrimary}
                disabled={saving || !form.name.trim() || !form.output_type.trim()}
                onClick={handleSave}
              >
                {saving ? 'Saving…' : editing ? 'Update' : 'Create'}
              </button>
              <button className={styles.btnGhost} onClick={closeForm}>
                Cancel
              </button>
            </div>
          </div>
        )}

        {/* Config list */}
        {configs.length === 0 && !showForm && (
          <p className={styles.dim}>No configured output devices yet.</p>
        )}
        <ul className={styles.list}>
          {configs.map((cfg) => (
            <li key={cfg.name} className={styles.cfgRow}>
              <div>
                <div className={styles.name}>{cfg.name}</div>
                <div className={styles.id}>{typeLabel(cfg.output_type)}{cfg.device ? ` — ${cfg.device}` : ''}</div>
              </div>
              <div style={{ display: 'flex', gap: '0.5rem', alignItems: 'center' }}>
                <span style={{ fontSize: '0.78rem', color: 'var(--text-faint)' }}>
                  {restartPending ? '⏳' : '✓'}
                </span>
                <button
                  className={styles.btnGhost}
                  onClick={() => openEditForm(cfg)}
                >
                  Edit
                </button>
                <button
                  className={styles.btnDanger}
                  onClick={() => setConfirmDelete(cfg.name)}
                >
                  Remove
                </button>
              </div>
            </li>
          ))}
        </ul>
      </div>

      {/* Confirmation dialog */}
      {confirmDelete && (
        <div className={styles.confirmOverlay} onClick={() => setConfirmDelete(null)}>
          <div className={styles.confirmBox} onClick={(e) => e.stopPropagation()}>
            <p className={styles.confirmText}>
              Remove device config <strong>{confirmDelete}</strong>? This does not
              restart MPD — the change takes effect after the next restart.
            </p>
            <div className={styles.confirmActions}>
              <button
                className={styles.btnGhost}
                onClick={() => setConfirmDelete(null)}
              >
                Cancel
              </button>
              <button
                className={styles.btnDanger}
                onClick={() => handleDelete(confirmDelete)}
              >
                Remove
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ════════ Bluetooth section ════════ */}
      <BluetoothSection />
    </div>
  )
}

// ── Bluetooth Section ──────────────────────────────────────────────────

/** Polling interval for scan results while a scan is active (ms). */
const SCAN_POLL_MS = 2000

function BluetoothSection() {
  const [btDevices, setBtDevices] = useState<BtDevice[]>([])
  const [btError, setBtError] = useState<string | null>(null)
  const [scanning, setScanning] = useState(false)
  const [scanResults, setScanResults] = useState<BtDevice[]>([])
  const [busyAddr, setBusyAddr] = useState<string | null>(null)
  const [inputStatus, setInputStatus] = useState<InputStatusResponse | null>(null)
  const [inputBusy, setInputBusy] = useState(false)

  const loadDevices = useCallback(async () => {
    try {
      const devices = await api.btDevices()
      setBtDevices(devices)
      setBtError(null)
    } catch (e) {
      // 503 = Bluetooth unavailable (no BlueZ, stub platform, init failure)
      const is503 = e instanceof Error && e.message.includes('503')
      if (!is503) {
        setBtError(e instanceof Error ? e.message : String(e))
      }
    }
  }, [])

  const loadInputStatus = useCallback(async () => {
    try {
      const status = await api.btInputStatus()
      setInputStatus(status)
    } catch {
      // ignore
    }
  }, [])

  // Initial load
  useEffect(() => {
    loadDevices()
    loadInputStatus()
  }, [loadDevices, loadInputStatus])

  // Start scanning
  const startScan = useCallback(async () => {
    setScanning(true)
    setBtError(null)
    try {
      await api.btScan(15)
    } catch (e) {
      setBtError(e instanceof Error ? e.message : String(e))
      setScanning(false)
    }
  }, [])

  // Poll scan results while scanning
  useEffect(() => {
    if (!scanning) return
    const poll = setInterval(async () => {
      try {
        const res = await api.btScanResults()
        setScanResults(res.devices)
        if (!res.active) {
          setScanning(false)
        }
      } catch {
        setScanning(false)
      }
    }, SCAN_POLL_MS)
    return () => clearInterval(poll)
  }, [scanning])

  const stopScan = useCallback(async () => {
    try {
      await api.btScanStop()
    } catch { /* ignore */ }
    setScanning(false)
  }, [])

  const handlePair = useCallback(async (address: string) => {
    setBusyAddr(address)
    setBtError(null)
    try {
      await api.btPair(address)
      await loadDevices()
    } catch (e) {
      setBtError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusyAddr(null)
    }
  }, [loadDevices])

  const handleConnect = useCallback(async (address: string) => {
    setBusyAddr(address)
    setBtError(null)
    try {
      await api.btConnect(address)
      await loadDevices()
    } catch (e) {
      setBtError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusyAddr(null)
    }
  }, [loadDevices])

  const handleWakeConnect = useCallback(async (address: string) => {
    setBusyAddr(address)
    setBtError(null)
    try {
      await api.btWakeConnect(address)
      await loadDevices()
    } catch (e) {
      setBtError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusyAddr(null)
    }
  }, [loadDevices])

  const handleDisconnect = useCallback(async (address: string) => {
    setBusyAddr(address)
    setBtError(null)
    try {
      await api.btDisconnect(address)
      await loadDevices()
    } catch (e) {
      setBtError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusyAddr(null)
    }
  }, [loadDevices])

  const handleForget = useCallback(async (address: string) => {
    setBusyAddr(address)
    setBtError(null)
    try {
      await api.btForget(address)
      await loadDevices()
    } catch (e) {
      setBtError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusyAddr(null)
    }
  }, [loadDevices])

  const handleRemoveOutput = useCallback(async (address: string) => {
    setBusyAddr(address)
    setBtError(null)
    try {
      await api.btRemoveOutput(address)
    } catch (e) {
      setBtError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusyAddr(null)
    }
  }, [])



  const handleToggleInput = useCallback(async () => {
    setInputBusy(true)
    setBtError(null)
    try {
      if (inputStatus?.enabled) {
        await api.btInputDisable()
      } else {
        await api.btInputEnable()
      }
      await loadInputStatus()
    } catch (e) {
      setBtError(e instanceof Error ? e.message : String(e))
    } finally {
      setInputBusy(false)
    }
  }, [inputStatus, loadInputStatus])

  // Combine known paired devices with scan results (dedup by address)
  const knownAddrs = new Set(btDevices.map((d) => d.address))
  const discoveredInScan = scanResults.filter((d) => !knownAddrs.has(d.address))
  const allDevices = [...btDevices, ...discoveredInScan]

  // If the backend returned 503, don't render the BT section at all
  // (non-Linux platform). We detect this by checking whether any paired
  // devices were returned after the initial load.
  if (btDevices.length === 0 && scanResults.length === 0 && btError === null) {
    // Still loading — show a placeholder
    return (
      <div className={styles.cfgSection}>
        <div>
          <span className={styles.eyebrow}>Bluetooth</span>
          <h3 className={styles.h}>Devices</h3>
        </div>
        <p className={styles.dim}>loading…</p>
      </div>
    )
  }

  return (
    <div className={styles.cfgSection}>
      <div>
        <span className={styles.eyebrow}>Bluetooth</span>
        <h3 className={styles.h}>Devices</h3>
      </div>

      {btError && <div className={styles.error}>{btError}</div>}

      {/* Scan controls */}
      <div className={styles.cfgToolbar}>
        {scanning ? (
          <>
            <span className={styles.dim}>Scanning…</span>
            <button className={styles.btnGhost} onClick={stopScan}>
              Stop scan
            </button>
          </>
        ) : (
          <button className={styles.addBtn} onClick={startScan}>
            Scan for devices
          </button>
        )}
      </div>

      {/* BT device list */}
      {allDevices.length === 0 && (
        <p className={styles.dim}>
          {scanning ? 'No devices found yet…' : 'No Bluetooth devices paired or discovered. Start a scan to find nearby speakers and headphones.'}
        </p>
      )}
      <ul className={styles.list}>
        {allDevices.map((d) => {
          const isBusy = busyAddr === d.address
          return (
            <li key={d.address} className={d.connected ? styles.rowOn : styles.rowOff}>
              <div>
                <div className={styles.name}>
                  {d.name ?? d.address}
                  {d.connected && <span className={styles.btBadge}>connected</span>}
                  {!d.connected && d.paired && <span className={styles.btBadge}>paired</span>}
                </div>
                <div className={styles.id}>
                  {d.address}
                  {d.rssi != null && <span> RSSI: {d.rssi}</span>}
                </div>
              </div>
              <div className={styles.btActions}>                {d.connected ? (
                  <>
                    <button
                      className={styles.btnGhost}
                      disabled={isBusy}
                      onClick={() => handleDisconnect(d.address)}
                    >
                      {isBusy ? '…' : 'Disconnect'}
                    </button>
                    <button
                      className={styles.btnGhost}
                      disabled={isBusy}
                      onClick={() => handleRemoveOutput(d.address)}
                    >
                      Remove output
                    </button>
                  </>
                ) : d.paired ? (
                  <>
                    <button
                      className={styles.off}
                      disabled={isBusy}
                      onClick={() => handleConnect(d.address)}
                    >
                      {isBusy ? '…' : 'Connect'}
                    </button>
                    <button
                      className={styles.on}
                      disabled={isBusy}
                      onClick={() => handleWakeConnect(d.address)}
                      title="Wake speaker from sleep and connect (with retries)"
                    >
                      {isBusy ? '…' : 'Wake & Connect'}
                    </button>
                    <button
                      className={styles.btnGhost}
                      disabled={isBusy}
                      onClick={() => handleForget(d.address)}
                    >
                      Forget
                    </button>
                  </>
                ) : (
                  <button
                    className={styles.on}
                    disabled={isBusy}
                    onClick={() => handlePair(d.address)}
                  >
                    {isBusy ? '…' : 'Pair'}
                  </button>
                )}
              </div>
            </li>
          )
        })}
      </ul>

      {/* A2DP Sink input toggle */}
      <div className={styles.btInputRow}>
        <div>
          <span className={styles.eyebrow}>Input</span>
          <h4 className={styles.h}>A2DP Sink</h4>
          <p className={styles.dim}>
            Allow phones and tablets to stream audio to this system.
          </p>
        </div>
        <button
          className={inputStatus?.enabled ? styles.on : styles.off}
          disabled={inputBusy}
          onClick={handleToggleInput}
        >
          {inputBusy
            ? '…'
            : inputStatus?.enabled
              ? inputStatus?.streaming
                ? 'Streaming'
                : 'Enabled'
              : 'Disabled'}
        </button>
      </div>
      {inputStatus?.streaming && (
        <div className={styles.btNote}>
          Audio is being streamed from a phone or tablet.
        </div>
      )}
    </div>
  )
}
