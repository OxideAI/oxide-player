import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import type { DeviceOutput } from '../types'
import { DspView, formatDspSettings } from './DspView'

const apiMock = vi.hoisted(() => ({
  dsp: vi.fn(),
  setDsp: vi.fn(),
  importDspText: vi.fn(),
  importDspUrl: vi.fn(),
}))

vi.mock('../api', () => ({ api: apiMock }))

describe('DspView preamp and imports', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    apiMock.dsp.mockResolvedValue([
      {
        device: 'DAC',
        mode: 'bit_perfect',
        target_rate: null,
        preset: 'balanced',
        preamp: 0,
        eq_bands: [],
      },
    ])
    apiMock.setDsp.mockResolvedValue(undefined)
    apiMock.importDspText.mockResolvedValue({
      preamp: -0.7,
      eq_bands: [{ type: 'peaking', freq: 1000, gain: 2, q: 1.2 }],
    })
    apiMock.importDspUrl.mockResolvedValue({
      preamp: -3,
      eq_bands: [{ type: 'high_shelf', freq: 8000, gain: 1, q: 0.7 }],
    })
  })

  it('saves preamp gain in the DSP profile', async () => {
    render(<DspView />)
    const preamp = await screen.findByRole('spinbutton', { name: 'Preamp gain (dB)' })
    fireEvent.change(preamp, { target: { value: '-6.5' } })
    fireEvent.click(screen.getByRole('button', { name: 'Apply' }))

    await waitFor(() => expect(apiMock.setDsp).toHaveBeenCalled())
    expect(apiMock.setDsp).toHaveBeenCalledWith(
      expect.objectContaining({
        device: 'DAC',
        preamp: -6.5,
        eq_bands: [],
      }),
    )
  })

  it('imports settings from an uploaded text file before Apply', async () => {
    render(<DspView />)
    await screen.findByRole('spinbutton', { name: 'Preamp gain (dB)' })
    const file = new File(
      ['Preamp: -0.7 dB\nFilter 1: ON PK Fc 1000 Hz Gain +2 dB Q 1.2\n'],
      'speaker-eq.txt',
      { type: 'text/plain' },
    )
    fireEvent.change(screen.getByLabelText('DSP settings file'), { target: { files: [file] } })

    await waitFor(() => expect(apiMock.importDspText).toHaveBeenCalled())
    expect(apiMock.importDspText).toHaveBeenCalledWith(await file.text())
    await waitFor(() =>
      expect(
        (screen.getByRole('spinbutton', { name: 'Preamp gain (dB)' }) as HTMLInputElement).value,
      ).toBe('-0.7'),
    )
    expect(screen.getByText(/Imported 1 filter/)).toBeTruthy()

    fireEvent.click(screen.getByRole('button', { name: 'Apply' }))
    await waitFor(() => expect(apiMock.setDsp).toHaveBeenCalledWith(
      expect.objectContaining({
        preamp: -0.7,
        eq_bands: [{ type: 'peaking', freq: 1000, gain: 2, q: 1.2 }],
      }),
    ))
  })

  it('imports settings from a URL', async () => {
    render(<DspView />)
    await screen.findByRole('spinbutton', { name: 'Preamp gain (dB)' })
    const url = screen.getByRole('textbox', { name: 'DSP settings URL' })
    fireEvent.change(url, { target: { value: 'https://example.test/eq.txt' } })
    fireEvent.click(screen.getByRole('button', { name: 'Import URL' }))

    await waitFor(() => expect(apiMock.importDspUrl).toHaveBeenCalledWith('https://example.test/eq.txt'))
    await waitFor(() =>
      expect(
        (screen.getByRole('spinbutton', { name: 'Preamp gain (dB)' }) as HTMLInputElement).value,
      ).toBe('-3'),
    )
    expect(screen.getByText(/Imported 1 filter/)).toBeTruthy()
  })

  it('exports the parsed subset in an AutoEQ-compatible format', () => {
    expect(
      formatDspSettings({
        preamp: -0.7,
        eq_bands: [
          { type: 'peaking', freq: 1000, gain: 2, q: 1.2 },
          { type: 'low_shelf', freq: 80, gain: -3.5, q: 0.7 },
        ],
      }),
    ).toBe(
      'Preamp: -0.70 dB\n\n' +
        'Filter  1: ON PK Fc 1000.00 Hz Gain +2.00 dB Q 1.20\n' +
        'Filter  2: ON LS Fc 80.00 Hz Gain -3.50 dB Q 0.70\n',
    )
  })

  it('downloads the current profile as a text file', async () => {
    const originalCreate = URL.createObjectURL
    const originalRevoke = URL.revokeObjectURL
    const createObjectURL = vi.fn((_blob: Blob) => 'blob:test')
    const revokeObjectURL = vi.fn()
    Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: createObjectURL })
    Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: revokeObjectURL })
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {})

    try {
      render(<DspView />)
      await screen.findByRole('spinbutton', { name: 'Preamp gain (dB)' })
      fireEvent.click(screen.getByRole('button', { name: 'Export .txt' }))
      expect(createObjectURL).toHaveBeenCalledOnce()
      expect((createObjectURL.mock.calls[0][0] as Blob).type).toBe('text/plain;charset=utf-8')
      expect(click).toHaveBeenCalledOnce()
      expect(revokeObjectURL).toHaveBeenCalledWith('blob:test')
    } finally {
      click.mockRestore()
      if (originalCreate) {
        Object.defineProperty(URL, 'createObjectURL', {
          configurable: true,
          value: originalCreate,
        })
      } else {
        delete (URL as unknown as Record<string, unknown>).createObjectURL
      }
      if (originalRevoke) {
        Object.defineProperty(URL, 'revokeObjectURL', {
          configurable: true,
          value: originalRevoke,
        })
      } else {
        delete (URL as unknown as Record<string, unknown>).revokeObjectURL
      }
    }
  })
})

describe('DspView route verification', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    apiMock.dsp.mockResolvedValue([
      {
        device: 'hw:USB,0',
        mode: 'bit_perfect',
        target_rate: null,
        preset: 'balanced',
        preamp: 0,
        eq_bands: [],
      },
    ])
    apiMock.setDsp.mockResolvedValue({
      device: 'hw:USB,0',
      persisted: true,
      reload_confirmed: true,
      active: true,
    })
  })
  const output = (patch: Partial<DeviceOutput> = {}): DeviceOutput => ({
    id: 1,
    name: 'USB DAC',
    enabled: true,
    role: 'playback',
    selectable: true,
    selection_key: 'alsa:USB DAC',
    configured: true,
    available: true,
    connected: null,
    active: true,
    dsp_supported: true,
    dsp_enabled: false,
    dsp_device: 'hw:USB,0',
    ...patch,
  })

  it('shows unsupported outputs before exposing the editor', async () => {
    render(<DspView selectedOutput={output({ dsp_supported: false, diagnostic_code: 'unsupported_output_type' })} />)
    expect(await screen.findByText('This output does not support DSP.')).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Apply' })).toBeNull()
  })

  it('distinguishes a persisted but unconfirmed reload and allows reconcile retry', async () => {
    apiMock.setDsp
      .mockResolvedValueOnce({
        device: 'hw:USB,0',
        persisted: true,
        reload_confirmed: false,
        active: false,
        reload_error: 'CamillaDSP reload refused',
      })
      .mockResolvedValueOnce({
        device: 'hw:USB,0',
        persisted: true,
        reload_confirmed: true,
        active: true,
      })
    const onRefresh = vi.fn()
    render(<DspView selectedOutput={output()} onRefresh={onRefresh} />)
    fireEvent.click(await screen.findByRole('button', { name: 'Edit EQ and resampling' }))
    const preamp = await screen.findByRole('spinbutton', { name: 'Preamp gain (dB)' })
    fireEvent.change(preamp, { target: { value: '-4' } })
    fireEvent.click(screen.getByRole('button', { name: 'Apply' }))
    await waitFor(() => expect(apiMock.setDsp).toHaveBeenCalled())
    expect(await screen.findByText(/Saved, but the DSP route is not confirmed/)).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Apply' }))
    expect(await screen.findByText('Saved and reload-confirmed active.')).toBeTruthy()
    expect(onRefresh).toHaveBeenCalledTimes(2)
  })

  it('requires an explicit choice before switching a dirty output draft', async () => {
    const first = output()
    const second = output({ id: 2, name: 'Headphones', selection_key: 'alsa:Headphones', dsp_device: 'hw:Headphones,0' })
    const onSelectOutput = vi.fn()
    const view = render(<DspView selectedOutput={first} onSelectOutput={onSelectOutput} />)
    fireEvent.click(await screen.findByRole('button', { name: 'Edit EQ and resampling' }))
    const preamp = await screen.findByRole('spinbutton', { name: 'Preamp gain (dB)' })
    fireEvent.change(preamp, { target: { value: '-2' } })
    view.rerender(<DspView selectedOutput={second} onSelectOutput={onSelectOutput} />)
    expect(await screen.findByRole('dialog')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Keep editing' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Discard and switch' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Discard and switch' }))
    expect(onSelectOutput).toHaveBeenLastCalledWith(second.selection_key)
  })
})
