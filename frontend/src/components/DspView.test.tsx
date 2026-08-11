import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
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
