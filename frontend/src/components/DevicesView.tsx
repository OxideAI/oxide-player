import { useCallback, useEffect, useMemo, useState } from 'react'
import type { BtDevice, DeviceConfig, DeviceOutput, InputStatusResponse, UsbAudioDevice } from '../types'
import { btDisplayName } from '../types'
import { api, ApiError } from '../api'
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

export interface DevicesViewProps {
  /** U2 may provide the ConfigView-owned snapshot; standalone use fetches it. */
  outputs?: DeviceOutput[]
  configs?: DeviceConfig[]
  selectedOutputKey?: string | null
  onSelectOutput?: (selectionKey: string) => void
  onRefresh?: () => Promise<void> | void
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}


function outputStatus(output: DeviceOutput): string {
  if (output.diagnostic_code === 'reload_error') return 'Provisioning failed — retry MPD reload'
  if (output.diagnostic_code === 'disconnected') return 'Bluetooth disconnected'
  if (output.diagnostic_code === 'inactive') return 'Disabled'
  if (output.diagnostic_code === 'missing_profile') return 'DSP profile missing'
  if (output.diagnostic_code === 'unsupported_output_type') return 'Playback output · DSP unavailable'
  if (output.connected === false) return 'Bluetooth disconnected'
  if (output.enabled) return output.active ? 'Ready for playback' : 'Configured · waiting for MPD'
  return 'Disabled'
}

function outputTypeLabel(type: string): string {
  const known: Record<string, string> = {
    alsa: 'USB / DAC',
    pulse: 'PulseAudio',
    fifo: 'Visualizer / FIFO',
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
  return known[type] ?? type
}

function isBluetoothConfig(config: DeviceConfig, address: string): boolean {
  const device = config.device?.toUpperCase() ?? ''
  return device.includes(`DEV=${address.toUpperCase()}`) || device.includes(address.toUpperCase())
}

export function DevicesView({
  outputs: providedOutputs,
  configs: providedConfigs,
  selectedOutputKey = null,
  onSelectOutput,
  onRefresh,
}: DevicesViewProps = {}) {
  const [devices, setDevices] = useState<DeviceOutput[]>(providedOutputs ?? [])
  const [deviceLoading, setDeviceLoading] = useState(providedOutputs === undefined)
  const [deviceError, setDeviceError] = useState<string | null>(null)
  const [deviceStale, setDeviceStale] = useState(false)

  const [configs, setConfigs] = useState<DeviceConfig[]>(providedConfigs ?? [])
  const [configLoading, setConfigLoading] = useState(providedConfigs === undefined)
  const [configError, setConfigError] = useState<string | null>(null)
  const [configStale, setConfigStale] = useState(false)
  const [busy, setBusy] = useState<number | null>(null)
  const [restartPending, setRestartPending] = useState(false)
  const [restarting, setRestarting] = useState(false)

  const [showForm, setShowForm] = useState(false)
  const [editing, setEditing] = useState<string | null>(null)
  const [form, setForm] = useState<FormData>(emptyForm)
  const [formError, setFormError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const [usbDevices, setUsbDevices] = useState<UsbAudioDevice[]>([])
  const [usbScanning, setUsbScanning] = useState(false)
  const [usbError, setUsbError] = useState<string | null>(null)

  useEffect(() => {
    if (providedOutputs !== undefined) {
      setDevices(providedOutputs)
      setDeviceLoading(false)
      setDeviceStale(false)
    }
  }, [providedOutputs])

  useEffect(() => {
    if (providedConfigs !== undefined) {
      setConfigs(providedConfigs)
      setConfigLoading(false)
      setConfigStale(false)
    }
  }, [providedConfigs])

  const loadDevices = useCallback(async () => {
    if (providedOutputs !== undefined) return
    setDeviceLoading(true)
    setDeviceError(null)
    try {
      setDevices(await api.devices())
      setDeviceStale(false)
    } catch (error) {
      setDeviceError(errorMessage(error))
      setDeviceStale(true)
    } finally {
      setDeviceLoading(false)
    }
  }, [providedOutputs])

  const loadConfigs = useCallback(async () => {
    if (providedConfigs !== undefined) return
    setConfigLoading(true)
    setConfigError(null)
    try {
      const next = await api.deviceConfigs()
      setConfigs(next)
      setRestartPending(next.some((config) => config.restart_pending))
      setConfigStale(false)
    } catch (error) {
      setConfigError(errorMessage(error))
      setConfigStale(true)
    } finally {
      setConfigLoading(false)
    }
  }, [providedConfigs])

  const refreshSnapshots = useCallback(async () => {
    await Promise.all([loadDevices(), loadConfigs()])
    await onRefresh?.()
  }, [loadConfigs, loadDevices, onRefresh])

  useEffect(() => {
    void Promise.all([loadDevices(), loadConfigs()])
  }, [loadDevices, loadConfigs])

  const toggle = async (device: DeviceOutput) => {
    setBusy(device.id)
    setActionError(null)
    try {
      if (device.enabled) await api.disableDevice(device.id)
      else await api.enableDevice(device.id)
      await refreshSnapshots()
    } catch (error) {
      setActionError(`Could not update ${device.name}: ${errorMessage(error)}`)
    } finally {
      setBusy(null)
    }
  }

  const toggleDsp = async (device: DeviceOutput) => {
    if (device.dsp_supported !== true) return
    setBusy(device.id)
    setActionError(null)
    try {
      if (device.dsp_enabled) await api.disableDeviceDsp(device.id)
      else await api.enableDeviceDsp(device.id)
      await refreshSnapshots()
    } catch (error) {
      setActionError(`Could not update DSP for ${device.name}: ${errorMessage(error)}`)
    } finally {
      setBusy(null)
    }
  }

  const openAddForm = (initial?: Partial<FormData>) => {
    setEditing(null)
    setForm({ ...emptyForm, ...initial })
    setFormError(null)
    setShowForm(true)
  }

  const openUsbForm = (device: UsbAudioDevice) => {
    openAddForm({
      name: device.name,
      output_type: 'alsa',
      device: device.alsa_device,
    })
  }

  const openEditForm = (config: DeviceConfig) => {
    setEditing(config.name)
    setForm({
      name: config.name,
      output_type: config.output_type,
      device: config.device ?? '',
      format: config.format ?? '',
      mixer_type: config.mixer_type ?? '',
      mixer_device: config.mixer_device ?? '',
      dop: config.dop,
    })
    setFormError(null)
    setShowForm(true)
  }

  const scanUsbDevices = async () => {
    setUsbScanning(true)
    setUsbError(null)
    try {
      setUsbDevices(await api.usbDevices())
    } catch (error) {
      setUsbError(errorMessage(error))
      setUsbDevices([])
    } finally {
      setUsbScanning(false)
    }
  }

  const closeForm = () => {
    setShowForm(false)
    setEditing(null)
    setForm(emptyForm)
    setFormError(null)
  }

  const saveForm = async () => {
    setFormError(null)
    setSaving(true)
    try {
      if (editing) await api.updateDeviceConfig(editing, form)
      else await api.createDeviceConfig(form)
      closeForm()
      await refreshSnapshots()
    } catch (error) {
      setFormError(errorMessage(error))
    } finally {
      setSaving(false)
    }
  }

  const deleteConfig = async (name: string) => {
    setConfirmDelete(null)
    setActionError(null)
    try {
      await api.deleteDeviceConfig(name)
      await refreshSnapshots()
    } catch (error) {
      setActionError(`Could not remove ${name}: ${errorMessage(error)}`)
    }
  }

  const restartMpd = async () => {
    setRestarting(true)
    setActionError(null)
    try {
      await api.restartMpd()
      setRestartPending(false)
      await refreshSnapshots()
    } catch (error) {
      setActionError(`MPD restart failed: ${errorMessage(error)}`)
    } finally {
      setRestarting(false)
    }
  }

  const playbackOutputs = useMemo(
    () => devices.filter((device) => (device.role ?? 'playback') === 'playback' && device.selectable !== false),
    [devices],
  )
  const advancedOutputs = useMemo(
    () => devices.filter((device) => !playbackOutputs.includes(device)),
    [devices, playbackOutputs],
  )

  return (
    <div className={styles.wrap}>
      <div>
        <span className={styles.eyebrow}>Playback destinations</span>
        <h2 className={styles.h}>Output devices</h2>
        <p className={styles.dim}>Choose a DAC or configured Bluetooth speaker. Output changes restart MPD automatically; system and visualizer outputs stay out of this list.</p>
      </div>

      {deviceError && <div className={styles.error} role="alert">Output status could not be refreshed: {deviceError}</div>}
      {configError && <div className={styles.error} role="alert">Output configuration could not be refreshed: {configError}</div>}
      {actionError && <div className={styles.error} role="alert">{actionError}</div>}
      {(deviceStale || configStale) && <p className={styles.stale} role="status">Showing the last known output snapshot. Retry to reconcile current state.</p>}

      {restartPending && (
        <div className={styles.banner} role="status">
          <span>MPD reload is pending for the changed output{configs.length > 1 ? 's' : ''}.</span>
          <button className={styles.bannerBtn} disabled={restarting} onClick={() => void restartMpd()}>
            {restarting ? 'Restarting…' : 'Restart MPD'}
          </button>
        </div>
      )}
      {configs.some((config) => config.include_warning) && (
        <div className={styles.error} role="alert">
          MPD is not loading managed output fragments. Add the configured <code>include</code> directive before retrying.
        </div>
      )}

      <section className={styles.primarySection} aria-labelledby="playback-outputs-heading">
        <div className={styles.sectionHead}>
          <div>
            <span className={styles.eyebrow}>Playback</span>
            <h3 className={styles.h} id="playback-outputs-heading">Configured outputs</h3>
          </div>
          <button className={styles.btnGhost} disabled={restarting} onClick={() => void restartMpd()}>
            {restarting ? 'Restarting…' : 'Restart MPD'}
          </button>
        </div>
        {deviceLoading && <p className={styles.dim}>Loading output status…</p>}
        {!deviceLoading && playbackOutputs.length === 0 && <p className={styles.dim}>No configured playback outputs are visible in MPD yet.</p>}
        <ul className={styles.list}>
          {playbackOutputs.map((device) => {
            const key = device.selection_key || `managed:${device.name}`
            const selected = selectedOutputKey === key
            const config = configs.find((candidate) => candidate.name === device.name)
            const kind = device.configured === false
              ? 'Runtime output'
              : config?.device?.toLowerCase().startsWith('bluealsa:')
                ? 'Bluetooth playback'
                : outputTypeLabel(config?.output_type ?? 'alsa')
            return (
              <li key={`${device.id}:${key}`} className={device.enabled ? styles.rowOn : styles.rowOff}>
                <button className={styles.outputSelect} aria-pressed={selected} onClick={() => onSelectOutput?.(key)}>
                  <span>
                    <span className={styles.name}>{device.name}</span>
                    <span className={styles.outputKind}>{kind}</span>
                    <span className={styles.id}>{outputStatus(device)}</span>
                  </span>
                  {selected && <span className={styles.btBadge}>selected</span>}
                </button>
                <div className={styles.outputActions}>
                  <button
                    className={device.dsp_enabled ? styles.dspOn : styles.dspOff}
                    disabled={busy === device.id || device.dsp_supported !== true}
                    title={device.dsp_supported === true ? 'Route this output through CamillaDSP' : device.dsp_reason}
                    aria-label={device.dsp_enabled ? `Disable DSP for ${device.name}` : `Enable DSP for ${device.name}`}
                    onClick={() => void toggleDsp(device)}
                  >
                    {device.dsp_enabled ? 'DSP on' : 'DSP off'}
                  </button>
                  <button className={device.enabled ? styles.on : styles.off} disabled={busy === device.id} onClick={() => void toggle(device)}>
                    {device.enabled ? 'Enabled' : 'Disabled'}
                  </button>
                </div>
              </li>
            )
          })}
        </ul>
      </section>

      {advancedOutputs.length > 0 && (
        <details className={styles.advancedBlock}>
          <summary className={styles.advancedSummary}>Technical and system outputs</summary>
          <p className={styles.dim}>These outputs are inspectable but cannot be selected as listening destinations.</p>
          <ul className={styles.list}>
            {advancedOutputs.map((device) => (
              <li key={device.id} className={styles.cfgRow}>
                <div><div className={styles.name}>{device.name}</div><div className={styles.id}>{device.role ?? 'unknown'} · {device.technical_detail ?? 'not selectable'}</div></div>
              </li>
            ))}
          </ul>
        </details>
      )}

      <UsbSection devices={usbDevices} scanning={usbScanning} error={usbError} onScan={() => void scanUsbDevices()} onSelect={openUsbForm} />

      <AdvancedConfig
        configs={configs}
        configLoading={configLoading}
        configStale={configStale}
        showForm={showForm}
        editing={editing}
        form={form}
        formError={formError}
        saving={saving}
        confirmDelete={confirmDelete}
        setForm={setForm}
        setConfirmDelete={setConfirmDelete}
        openAddForm={openAddForm}
        openEditForm={openEditForm}
        closeForm={closeForm}
        saveForm={saveForm}
        deleteConfig={deleteConfig}
      />

      <BluetoothSection configs={configs} onRefresh={refreshSnapshots} />
    </div>
  )
}
interface UsbSectionProps {
  devices: UsbAudioDevice[]
  scanning: boolean
  error: string | null
  onScan: () => void
  onSelect: (device: UsbAudioDevice) => void
}

function UsbSection({ devices, scanning, error, onScan, onSelect }: UsbSectionProps) {
  return (
    <section className={styles.advancedBlock} aria-labelledby="usb-dac-heading">
      <div className={styles.sectionHead}>
        <div>
          <span className={styles.eyebrow}>USB audio</span>
          <h3 className={styles.h} id="usb-dac-heading">USB DAC discovery</h3>
        </div>
        <button className={styles.addBtn} disabled={scanning} onClick={onScan}>
          {scanning ? 'Scanning…' : 'Scan USB audio devices'}
        </button>
      </div>
      <p className={styles.dim}>Scan ALSA playback hardware, then open the device details to configure the MPD output.</p>
      {error && <div className={styles.error} role="alert">USB audio scan failed: {error}</div>}
      {!error && devices.length === 0 && <p className={styles.dim}>No USB DACs found yet. Connect the DAC and scan again.</p>}
      <ul className={styles.list}>
        {devices.map((device) => (
          <li key={device.id} className={styles.cfgRow}>
            <div>
              <div className={styles.name}>{device.name}</div>
              <div className={styles.id}>Card {device.card}, device {device.device} · {device.alsa_device}</div>
            </div>
            <button className={styles.btnPrimary} onClick={() => onSelect(device)}>Configure device</button>
          </li>
        ))}
      </ul>
    </section>
  )
}


interface AdvancedConfigProps {
  configs: DeviceConfig[]
  configLoading: boolean
  configStale: boolean
  showForm: boolean
  editing: string | null
  form: FormData
  formError: string | null
  saving: boolean
  confirmDelete: string | null
  setForm: (value: FormData | ((previous: FormData) => FormData)) => void
  setConfirmDelete: (name: string | null) => void
  openAddForm: () => void
  openEditForm: (config: DeviceConfig) => void
  closeForm: () => void
  saveForm: () => Promise<void>
  deleteConfig: (name: string) => Promise<void>
}

function AdvancedConfig(props: AdvancedConfigProps) {
  const { configs, configLoading, configStale, showForm, editing, form, formError, saving, confirmDelete, setForm, setConfirmDelete, openAddForm, openEditForm, closeForm, saveForm, deleteConfig } = props
  const [openMenu, setOpenMenu] = useState<string | null>(null)
  return (
    <details className={styles.advancedBlock} open={showForm || undefined}>
      <summary className={styles.advancedSummary}>Advanced output configuration</summary>
      <p className={styles.dim}>Raw MPD type, device, format, mixer, and DoP fields are the DAC or output details used to build its managed MPD configuration. Changes restart MPD automatically.</p>
      {!showForm && <button className={styles.addBtn} onClick={openAddForm}>+ Add output configuration</button>}
      {configLoading && <p className={styles.dim}>Loading saved configurations…</p>}
      {configStale && <p className={styles.stale}>Saved configuration is stale.</p>}
      {showForm && (
        <div className={styles.form}>
          {formError && <div className={styles.error} role="alert">{formError}</div>}
          {(['name', 'output_type', 'device', 'format', 'mixer_type', 'mixer_device'] as const).map((field) => (
            <label key={field} className={styles.formRow}>
              <span className={styles.formLabel}>{field === 'output_type' ? 'Type' : field.replace('_', ' ')}</span>
              <input className={styles.formInput} value={form[field]} onChange={(event) => setForm((previous) => ({ ...previous, [field]: event.target.value }))} />
            </label>
          ))}
          <label className={styles.formCheck}><input type="checkbox" checked={form.dop} onChange={(event) => setForm((previous) => ({ ...previous, dop: event.target.checked }))} /><span className={styles.formLabel}>DoP (DSD over PCM)</span></label>
          <div className={styles.formActions}>
            <button className={styles.btnPrimary} disabled={saving || !form.name.trim() || !form.output_type.trim()} onClick={() => void saveForm()}>{saving ? 'Saving…' : editing ? 'Update' : 'Create'}</button>
            <button className={styles.btnGhost} onClick={closeForm}>Cancel</button>
          </div>
        </div>
      )}
      {!configLoading && configs.length === 0 && !showForm && <p className={styles.dim}>No managed output configurations.</p>}
      <ul className={styles.list}>
        {configs.map((config) => (
          <li key={config.name} className={styles.cfgRow}>
            <div><div className={styles.name}>{config.name}</div><div className={styles.id}>{config.output_type} · {config.device ?? 'default device'} · {config.format ?? 'MPD default'}</div></div>
            <div className={styles.btActions}><span className={config.restart_pending ? styles.pending : styles.ready}>{config.restart_pending ? 'reload pending' : 'saved'}</span><button className={styles.btnGhost} onClick={() => openEditForm(config)}>Edit</button><details className={styles.rowMenu} open={openMenu === config.name} onToggle={(event) => setOpenMenu(event.currentTarget.open ? config.name : null)}><summary className={styles.btnGhost}>More</summary><div aria-hidden={openMenu !== config.name}><button className={styles.btnDanger} onClick={() => setConfirmDelete(config.name)}>Remove output</button></div></details></div>
          </li>
        ))}
      </ul>
      {confirmDelete && (
        <div className={styles.confirmOverlay} role="presentation" onClick={() => setConfirmDelete(null)}>
          <div className={styles.confirmBox} role="dialog" aria-modal="true" onClick={(event) => event.stopPropagation()}>
            <p className={styles.confirmText}>Remove <strong>{confirmDelete}</strong> from managed playback outputs? MPD restarts automatically after removal.</p>
            <div className={styles.confirmActions}><button className={styles.btnGhost} onClick={() => setConfirmDelete(null)}>Cancel</button><button className={styles.btnDanger} onClick={() => void deleteConfig(confirmDelete)}>Remove output</button></div>
          </div>
        </div>
      )}
    </details>
  )
}

const SCAN_POLL_MS = 2000

type BluetoothErrorState = { message: string; unavailable: boolean } | null
interface BluetoothSectionProps { configs: DeviceConfig[]; onRefresh: () => Promise<void> }

function BluetoothSection({ configs, onRefresh }: BluetoothSectionProps) {
  const [btDevices, setBtDevices] = useState<BtDevice[]>([])
  const [btLoading, setBtLoading] = useState(true)
  const [btError, setBtError] = useState<BluetoothErrorState>(null)
  const [scanError, setScanError] = useState<string | null>(null)
  const [scanning, setScanning] = useState(false)
  const [scanResults, setScanResults] = useState<BtDevice[]>([])
  const [busyAddr, setBusyAddr] = useState<string | null>(null)
  const [inputStatus, setInputStatus] = useState<InputStatusResponse | null>(null)
  const [inputBusy, setInputBusy] = useState(false)
  const [inputError, setInputError] = useState<string | null>(null)
  const [confirm, setConfirm] = useState<{ action: 'forget' | 'remove'; device: BtDevice } | null>(null)
  const [advancedAddress, setAdvancedAddress] = useState<string | null>(null)

  const loadDevices = useCallback(async () => {
    setBtLoading(true)
    try {
      setBtDevices(await api.btDevices())
      setBtError(null)
    } catch (error) {
      setBtError({ message: errorMessage(error), unavailable: error instanceof ApiError && error.status === 503 })
    } finally {
      setBtLoading(false)
    }
  }, [])

  const loadInputStatus = useCallback(async () => {
    try { setInputStatus(await api.btInputStatus()); setInputError(null) }
    catch (error) { setInputError(errorMessage(error)) }
  }, [])

  useEffect(() => { void Promise.all([loadDevices(), loadInputStatus()]) }, [loadDevices, loadInputStatus])

  const startScan = useCallback(async () => {
    setScanResults([])
    setScanError(null)
    setScanning(true)
    try { await api.btScan(15) }
    catch (error) { setScanError(errorMessage(error)); setScanning(false) }
  }, [])

  useEffect(() => {
    if (!scanning) return
    const poll = setInterval(() => {
      void api.btScanResults().then((result) => {
        setScanResults(result.devices)
        if (!result.active) setScanning(false)
      }).catch((error) => {
        setScanError(errorMessage(error))
        setScanning(false)
      })
    }, SCAN_POLL_MS)
    return () => clearInterval(poll)
  }, [scanning])

  const stopScan = useCallback(async () => {
    try { await api.btScanStop() } catch (error) { setScanError(errorMessage(error)) }
    setScanning(false)
    setScanResults([])
  }, [])

  const runBluetoothAction = useCallback(async (address: string, action: () => Promise<unknown>, label: string) => {
    setBusyAddr(address)
    setBtError(null)
    try {
      await action()
      await loadDevices()
      await onRefresh()
    } catch (error) {
      setBtError({ message: `${label}: ${errorMessage(error)}`, unavailable: false })
    } finally { setBusyAddr(null) }
  }, [loadDevices, onRefresh])

  const handleInputToggle = async () => {
    setInputBusy(true); setInputError(null)
    try { if (inputStatus?.enabled) await api.btInputDisable(); else await api.btInputEnable(); await loadInputStatus() }
    catch (error) { setInputError(errorMessage(error)) }
    finally { setInputBusy(false) }
  }

  const configuredAddresses = useMemo(() => new Set(configs.flatMap((config) => btDevices.filter((device) => isBluetoothConfig(config, device.address)).map((device) => device.address))), [btDevices, configs])
  const knownAddresses = new Set(btDevices.map((device) => device.address))
  const candidates = [...btDevices, ...scanResults.filter((device) => !knownAddresses.has(device.address))]

  return (
    <section className={styles.btSection} aria-labelledby="bluetooth-outputs-heading">
      <div className={styles.sectionHead}><div><span className={styles.eyebrow}>Bluetooth playback</span><h3 className={styles.h} id="bluetooth-outputs-heading">Setup candidates</h3></div>{!btError?.unavailable && (scanning ? <button className={styles.btnGhost} onClick={() => void stopScan()}>Cancel scan</button> : <button className={styles.addBtn} onClick={() => void startScan()}>Scan for devices</button>)}</div>
      {btLoading && <p className={styles.dim}>Loading Bluetooth availability…</p>}
      {btError?.unavailable && <div className={styles.error} role="alert">Bluetooth is unavailable ({btError.message}). <button className={styles.btnGhost} onClick={() => void loadDevices()}>Retry Bluetooth</button></div>}
      {btError && !btError.unavailable && <div className={styles.error} role="alert">Bluetooth device status: {btError.message}</div>}
      {scanError && <div className={styles.error} role="alert">Scan failed: {scanError} <button className={styles.btnGhost} onClick={() => void startScan()}>Retry scan</button></div>}
      {scanning && <p className={styles.scanStatus} role="status">Scanning for nearby audio devices…</p>}
      {!btLoading && !btError && candidates.length === 0 && <p className={styles.dim}>{scanning ? 'No devices found yet…' : 'No paired or discovered Bluetooth setup candidates. Start a scan to find a speaker or headphones.'}</p>}
      <ul className={styles.list}>
        {candidates.map((device) => {
          const busyNow = busyAddr === device.address
          const isConfigured = configuredAddresses.has(device.address)
          return <li key={device.address} className={device.connected ? styles.rowOn : styles.rowOff}>
            <div><div className={styles.name}>{btDisplayName(device)}{device.connected && <span className={styles.btBadge}>connected</span>}{!device.connected && device.paired && <span className={styles.btBadge}>paired</span>}{isConfigured && <span className={styles.btBadge}>configured</span>}</div><div className={styles.id}>{device.address}{device.rssi != null && ` · RSSI ${device.rssi}`}</div>{device.connected && !isConfigured && <div className={styles.provisioning}>Connected, but no managed playback output is visible yet. Connect again after MPD reload if needed.</div>}</div>
            <div className={styles.btActions}>{device.connected ? <>{!isConfigured && <button className={styles.on} disabled={busyNow} onClick={() => void runBluetoothAction(device.address, () => api.btWakeConnect(device.address), 'Provisioning failed')}>{busyNow ? '…' : 'Retry provisioning'}</button>}<button className={styles.btnGhost} disabled={busyNow} onClick={() => void runBluetoothAction(device.address, () => api.btDisconnect(device.address), 'Disconnect failed')}>{busyNow ? '…' : 'Disconnect'}</button></> : device.paired ? <button className={styles.on} disabled={busyNow} onClick={() => void runBluetoothAction(device.address, () => api.btWakeConnect(device.address), 'Connect failed')}>{busyNow ? '…' : 'Connect'}</button> : <button className={styles.on} disabled={busyNow} onClick={() => void runBluetoothAction(device.address, () => api.btPair(device.address), 'Pair failed')}>{busyNow ? '…' : 'Pair'}</button>}<details className={styles.rowMenu} open={advancedAddress === device.address} onToggle={(event) => setAdvancedAddress(event.currentTarget.open ? device.address : null)}><summary className={styles.btnGhost}>More</summary><div aria-hidden={advancedAddress !== device.address}><button className={styles.btnDanger} disabled={busyNow} onClick={() => setConfirm({ action: device.connected ? 'remove' : 'forget', device })}>{device.connected ? 'Remove output' : 'Forget device'}</button></div></details></div>
          </li>
        })}
      </ul>

      {confirm && <div className={styles.confirmOverlay} role="presentation" onClick={() => setConfirm(null)}><div className={styles.confirmBox} role="dialog" aria-modal="true" onClick={(event) => event.stopPropagation()}><p className={styles.confirmText}>{confirm.action === 'forget' ? <>Forget <strong>{btDisplayName(confirm.device)}</strong>? Pairing and its saved device state will be removed.</> : <>Remove the managed playback output for <strong>{btDisplayName(confirm.device)}</strong>? The Bluetooth pairing remains available.</>}</p><div className={styles.confirmActions}><button className={styles.btnGhost} onClick={() => setConfirm(null)}>Cancel</button><button className={styles.btnDanger} onClick={() => { const next = confirm; setConfirm(null); void runBluetoothAction(next.device.address, next.action === 'forget' ? () => api.btForget(next.device.address) : () => api.btRemoveOutput(next.device.address), next.action === 'forget' ? 'Forget failed' : 'Remove output failed') }}>{confirm.action === 'forget' ? 'Forget device' : 'Remove output'}</button></div></div></div>}

      <details className={styles.advancedBlock}><summary className={styles.advancedSummary}>Bluetooth input and AirPlay</summary><div className={styles.btInputRow}><div><span className={styles.eyebrow}>Input</span><h4 className={styles.h}>A2DP Sink</h4><p className={styles.dim}>Allow phones and tablets to stream audio to this system.</p></div><button className={inputStatus?.enabled ? styles.on : styles.off} disabled={inputBusy} onClick={() => void handleInputToggle()}>{inputBusy ? '…' : inputStatus?.streaming ? 'Streaming' : inputStatus?.enabled ? 'Enabled' : 'Disabled'}</button></div>{inputError && <p className={styles.error}>Bluetooth input: {inputError}</p>}{inputStatus?.streaming && <div className={styles.btNote}>Audio is being streamed from a phone or tablet.</div>}<div className={styles.airplay}><span className={styles.eyebrow}>AirPlay</span><h4 className={styles.h}>iPhone and iPad input</h4><p className={styles.dim}>Select <strong>Oxide Player</strong> in the AirPlay output picker while both devices are on the same network.</p><span className={styles.btBadge}>LAN receiver</span></div></details>
    </section>
  )
}
