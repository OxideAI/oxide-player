import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { render, cleanup, waitFor, fireEvent } from '@testing-library/react'
import type { RadioStation } from '../types'

// Mock the api module so the component's list/add/edit/play/delete calls are
// observable. Only the methods RadioView touches need spies.
const listRadio = vi.fn<() => Promise<RadioStation[]>>(() => Promise.resolve([]))
const addRadio = vi.fn<(name: string, url: string, artwork: string | null) => Promise<RadioStation>>(() =>
  Promise.reject(new Error('unexpected addRadio call')),
)
const updateRadio = vi.fn<(id: string, name: string, artwork: string | null) => Promise<RadioStation>>(() =>
  Promise.reject(new Error('unexpected updateRadio call')),
)
const deleteRadio = vi.fn<(id: string) => Promise<unknown>>(() => Promise.resolve({}))
const playRadio = vi.fn<(id: string) => Promise<unknown>>(() => Promise.resolve({}))

vi.mock('../api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api')>()
  return {
    ...actual,
    api: {
      ...actual.api,
      listRadio: () => listRadio(),
      addRadio: (name: string, url: string, artwork: string | null) =>
        addRadio(name, url, artwork),
      updateRadio: (id: string, name: string, artwork: string | null) =>
        updateRadio(id, name, artwork),
      deleteRadio: (id: string) => deleteRadio(id),
      playRadio: (id: string) => playRadio(id),
    },
  }
})

import { RadioView } from './RadioView'

function makeStation(over: Partial<RadioStation> = {}): RadioStation {
  return {
    id: 'st-1',
    name: 'JFK Ibiza',
    url: 'https://stream.aiir.com/7dsjltmny8cvv',
    homepage: 'https://jfkibiza.es/',
    artwork: null,
    ...over,
  }
}

const noopProps = {
  nowPlayingUri: null,
  isPlaying: false,
}

describe('RadioView', () => {
  beforeEach(() => {
    listRadio.mockResolvedValue([makeStation()])
    addRadio.mockResolvedValue(makeStation())
  })

  afterEach(() => {
    cleanup()
    vi.clearAllMocks()
  })

  it('renders stations from the api', async () => {
    const { getByText } = render(<RadioView {...noopProps} />)
    await waitFor(() => expect(getByText('JFK Ibiza')).toBeTruthy())
    expect(listRadio).toHaveBeenCalledTimes(1)
  })

  it('rejects a non-http(s) URL without calling the api', async () => {
    const { getByLabelText, getByText } = render(<RadioView {...noopProps} />)
    await waitFor(() => expect(getByText('JFK Ibiza')).toBeTruthy())
    fireEvent.change(getByLabelText('Station name'), { target: { value: 'Nope' } })
    fireEvent.change(getByLabelText('Stream URL'), { target: { value: 'ftp://x.example/' } })
    fireEvent.click(getByText('Add station'))
    await waitFor(() =>
      expect(getByText('Station URL must start with http:// or https://')).toBeTruthy(),
    )
    expect(addRadio).not.toHaveBeenCalled()
  })

  it('rejects an empty name without calling the api', async () => {
    const { getByLabelText, getByText } = render(<RadioView {...noopProps} />)
    await waitFor(() => expect(getByText('JFK Ibiza')).toBeTruthy())
    fireEvent.change(getByLabelText('Stream URL'), {
      target: { value: 'https://example.com/stream' },
    })
    fireEvent.click(getByText('Add station'))
    await waitFor(() => expect(getByText('Station name is required.')).toBeTruthy())
    expect(addRadio).not.toHaveBeenCalled()
  })

  it('adds a trimmed station and refreshes the list', async () => {
    const { getByLabelText, getByText } = render(<RadioView {...noopProps} />)
    await waitFor(() => expect(getByText('JFK Ibiza')).toBeTruthy())
    fireEvent.change(getByLabelText('Station name'), { target: { value: '  Cool Radio  ' } })
    fireEvent.change(getByLabelText('Stream URL'), {
      target: { value: '  https://example.com/stream  ' },
    })
    fireEvent.change(getByLabelText('Artwork URL'), {
      target: { value: '  https://example.com/art.jpg  ' },
    })
    fireEvent.click(getByText('Add station'))
    await waitFor(() =>
      expect(addRadio).toHaveBeenCalledWith(
        'Cool Radio',
        'https://example.com/stream',
        'https://example.com/art.jpg',
      ),
    )
    expect(listRadio).toHaveBeenCalledTimes(2)
  })

  it('edits a station name and artwork', async () => {
    updateRadio.mockResolvedValue(
      makeStation({ name: 'Late Night FM', artwork: 'https://example.com/night.jpg' }),
    )
    const { getByLabelText, getByText } = render(<RadioView {...noopProps} />)
    await waitFor(() => expect(getByText('JFK Ibiza')).toBeTruthy())

    fireEvent.click(getByText('Edit'))
    fireEvent.change(getByLabelText('Edit station name for JFK Ibiza'), {
      target: { value: '  Late Night FM  ' },
    })
    fireEvent.change(getByLabelText('Artwork URL for JFK Ibiza'), {
      target: { value: '  https://example.com/night.jpg  ' },
    })
    fireEvent.click(getByText('Save changes'))

    await waitFor(() =>
      expect(updateRadio).toHaveBeenCalledWith(
        'st-1',
        'Late Night FM',
        'https://example.com/night.jpg',
      ),
    )
    expect(listRadio).toHaveBeenCalledTimes(2)
  })

  it('rejects invalid artwork URLs without calling the api', async () => {
    const { getByLabelText, getByText } = render(<RadioView {...noopProps} />)
    await waitFor(() => expect(getByText('JFK Ibiza')).toBeTruthy())
    fireEvent.click(getByText('Edit'))
    fireEvent.change(getByLabelText('Artwork URL for JFK Ibiza'), {
      target: { value: 'ftp://example.com/art.jpg' },
    })
    fireEvent.click(getByText('Save changes'))
    await waitFor(() =>
      expect(getByText('Artwork URL must start with http:// or https://')).toBeTruthy(),
    )
    expect(updateRadio).not.toHaveBeenCalled()
  })

  it('plays a station on Play click', async () => {
    const { getByText } = render(<RadioView {...noopProps} />)
    await waitFor(() => expect(getByText('JFK Ibiza')).toBeTruthy())
    fireEvent.click(getByText('Play'))
    expect(playRadio).toHaveBeenCalledWith('st-1')
  })

  it('disables Play and shows Playing for the live station', async () => {
    const { getByText } = render(
      <RadioView nowPlayingUri="https://stream.aiir.com/7dsjltmny8cvv" isPlaying />,
    )
    await waitFor(() => expect(getByText('Playing')).toBeTruthy())
    const play = getByText('Playing') as HTMLButtonElement
    expect(play.disabled).toBe(true)
    expect(playRadio).not.toHaveBeenCalled()
  })

  it('deletes a station and refreshes the list', async () => {
    const { getByText } = render(<RadioView {...noopProps} />)
    await waitFor(() => expect(getByText('JFK Ibiza')).toBeTruthy())
    fireEvent.click(getByText('Delete'))
    await waitFor(() => expect(deleteRadio).toHaveBeenCalledWith('st-1'))
    expect(listRadio).toHaveBeenCalledTimes(2)
  })

  it('renders the empty state when there are no stations', async () => {
    listRadio.mockResolvedValue([])
    const { getByText } = render(<RadioView {...noopProps} />)
    await waitFor(() => expect(getByText('No stations yet — add one above.')).toBeTruthy())
  })

  it('surfaces api errors', async () => {
    listRadio.mockRejectedValue(new Error('boom'))
    const { getByText } = render(<RadioView {...noopProps} />)
    await waitFor(() => expect(getByText('boom')).toBeTruthy())
  })
})
