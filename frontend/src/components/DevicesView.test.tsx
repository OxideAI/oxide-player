import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react'
import type { api as apiClient } from '../api'
import type { BtDevice, DeviceConfig, InputStatusResponse, OutputDevice } from '../types'

const listDevices = vi.fn<() => Promise<OutputDevice[]>>()
const listConfigs = vi.fn<() => Promise<DeviceConfig[]>>()
const listBluetoothDevices = vi.fn<() => Promise<BtDevice[]>>()
const inputStatus = vi.fn<() => Promise<InputStatusResponse>>()
const wakeConnect = vi.fn<(address: string) => Promise<unknown>>()
const disconnect = vi.fn<(address: string) => Promise<unknown>>()

vi.mock('../api', async (importOriginal) => {
  const actual = await importOriginal<{ api: typeof apiClient }>()
  return {
    api: {
      ...actual.api,
      devices: () => listDevices(),
      deviceConfigs: () => listConfigs(),
      btDevices: () => listBluetoothDevices(),
      btInputStatus: () => inputStatus(),
      btWakeConnect: (address: string) => wakeConnect(address),
      btDisconnect: (address: string) => disconnect(address),
    },
  }
})

import { DevicesView } from './DevicesView'

function makeDevice(over: Partial<BtDevice> = {}): BtDevice {
  return {
    address: 'AA:BB:CC:DD:EE:FF',
    name: 'Living Room Speaker',
    alias: null,
    class: null,
    icon: null,
    rssi: -42,
    connected: false,
    paired: true,
    trusted: false,
    ...over,
  }
}

describe('DevicesView Bluetooth actions', () => {
  beforeEach(() => {
    listDevices.mockResolvedValue([])
    listConfigs.mockResolvedValue([])
    inputStatus.mockResolvedValue({ enabled: false, streaming: false })
    wakeConnect.mockResolvedValue({})
    disconnect.mockResolvedValue({})
  })

  afterEach(() => {
    cleanup()
    vi.clearAllMocks()
  })

  it('shows Connect for a paired disconnected device and wakes it on click', async () => {
    listBluetoothDevices.mockResolvedValue([makeDevice()])

    const { getByRole, queryByRole } = render(<DevicesView />)
    await waitFor(() => expect(getByRole('button', { name: 'Connect' })).toBeTruthy())

    expect(queryByRole('button', { name: 'Wake & Connect' })).toBeNull()
    expect(queryByRole('button', { name: 'Disconnect' })).toBeNull()

    fireEvent.click(getByRole('button', { name: 'Connect' }))
    await waitFor(() => expect(wakeConnect).toHaveBeenCalledWith('AA:BB:CC:DD:EE:FF'))
  })

  it('shows Disconnect for a connected device', async () => {
    listBluetoothDevices.mockResolvedValue([makeDevice({ connected: true })])

    const { getByRole, queryByRole } = render(<DevicesView />)
    await waitFor(() => expect(getByRole('button', { name: 'Disconnect' })).toBeTruthy())

    expect(queryByRole('button', { name: 'Connect' })).toBeNull()
    fireEvent.click(getByRole('button', { name: 'Disconnect' }))
    await waitFor(() => expect(disconnect).toHaveBeenCalledWith('AA:BB:CC:DD:EE:FF'))
  })
})
