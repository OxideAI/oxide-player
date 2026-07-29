import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { render, cleanup, waitFor, fireEvent } from '@testing-library/react'
import { Track } from '../types'

// Mock the api module so the component's library load and playback calls are
// observable. Only the methods LibraryView actually touches need real spies.
const clearAndPlay = vi.fn<(tracks: unknown) => Promise<unknown>>(() => Promise.resolve({}))
const play = vi.fn<(...args: unknown[]) => Promise<unknown>>(() => Promise.resolve({}))
const library = vi.fn<() => Promise<Track[]>>()

// Mock only the `api` object (network calls) while keeping the real
// `toPlayRef` helper, so the wire-envelope assertions exercise actual code.
vi.mock('../api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api')>()
  return {
    ...actual,
    api: {
      library: () => library(),
      clearAndPlay: (tracks: unknown) => clearAndPlay(tracks),
      play: (...args: unknown[]) => play(...args),
      coverUrl: (key: string) => `/cover/${key}`,
    },
  }
})

import { LibraryView } from '../components/LibraryView'

function makeTrack(over: Partial<Track> = {}): Track {
  return {
    id: 7,
    uri: 'Diana Krall - The Girl In The Other Room/01 Diana Krall - Stop This World.m4a',
    path: 'Diana Krall - The Girl In The Other Room/01 Diana Krall - Stop This World.m4a',
    title: 'Stop This World',
    artist: 'Diana Krall',
    album: 'The Girl In The Other Room',
    has_cover: false,
    cover_key: null,
    format: 'M4A',
    sample_rate: 48000,
    bit_depth: 24,
    channels: 2,
    duration: 239,
    cue_start: null,
    genre: null,
    year: null,
    track: 1,
    album_artist: null,
    cue_index: null,
    start_time: null,
    end_time: null,
    file_mtime: null,
    source: null,
    ...over,
  }
}

const noop = () => Promise.resolve()

const folderKey = (uri: string) => uri.slice(0, uri.lastIndexOf('/'))

function renderAlbumView(tracks: Track[], props: Partial<Parameters<typeof LibraryView>[0]> = {}) {
  // Passing `album` equal to the tracks' folder key opens the album view
  // directly, so the track rows are rendered without an extra folder click.
  return render(
    <LibraryView
      refreshToken={0}
      onRefresh={noop}
      onRescanArt={noop}
      nowPlayingUri={null}
      nowPlayingId={null}
      isPlaying={false}
      album={folderKey(tracks[0].uri)}
      onAlbumChange={() => {}}
      {...props}
    />,
  )
}

// Find a track row by its stable data-track-id, waiting for the async library
// load to render it.
async function rowForTrack(container: HTMLElement, id: number): Promise<HTMLElement> {
  return waitFor(() => {
    const row = container.querySelector<HTMLElement>(`li[data-track-id="${id}"]`)
    if (!row) throw new Error(`row for track ${id} not rendered yet`)
    return row
  })
}

describe('LibraryView track click (issue #32)', () => {
  beforeEach(() => {
    clearAndPlay.mockClear()
    clearAndPlay.mockResolvedValue({})
    play.mockClear()
  })

  afterEach(() => cleanup())

  it('clears the queue and plays the clicked track (not append via api.play)', async () => {
    const track = makeTrack()
    library.mockResolvedValue([track])

    const { container } = renderAlbumView([track])
    fireEvent.click(await rowForTrack(container, track.id))

    await waitFor(() => expect(clearAndPlay).toHaveBeenCalledTimes(1))
    // Regression guard for #32: must NOT use the append-to-queue api.play path.
    expect(play).not.toHaveBeenCalled()
  })

  it('sends the single-track play envelope { uri, start, end, track_id }', async () => {
    const track = makeTrack({ id: 42, start_time: 12.5, end_time: 200 })
    library.mockResolvedValue([track])

    const { container } = renderAlbumView([track])
    fireEvent.click(await rowForTrack(container, track.id))

    await waitFor(() =>
      expect(clearAndPlay).toHaveBeenCalledWith([
        { uri: track.uri, start: 12.5, end: 200, track_id: 42 },
      ]),
    )
  })

  it('maps null cue times to undefined in the play envelope', async () => {
    const track = makeTrack({ id: 3, start_time: null, end_time: null })
    library.mockResolvedValue([track])

    const { container } = renderAlbumView([track])
    fireEvent.click(await rowForTrack(container, track.id))

    await waitFor(() =>
      expect(clearAndPlay).toHaveBeenCalledWith([
        { uri: track.uri, start: undefined, end: undefined, track_id: 3 },
      ]),
    )
  })

  it('plays the clicked track, not the first, when the album has several', async () => {
    const first = makeTrack({
      id: 1,
      track: 1,
      title: 'Stop This World',
      uri: 'Diana Krall - The Girl In The Other Room/01 Stop This World.m4a',
    })
    const third = makeTrack({
      id: 3,
      track: 3,
      title: 'Temptation',
      uri: 'Diana Krall - The Girl In The Other Room/03 Temptation.m4a',
    })
    library.mockResolvedValue([first, third])

    const { container } = renderAlbumView([first, third])
    fireEvent.click(await rowForTrack(container, third.id))

    await waitFor(() =>
      expect(clearAndPlay).toHaveBeenCalledWith([
        { uri: third.uri, start: undefined, end: undefined, track_id: 3 },
      ]),
    )
  })

  it('marks the clicked row as playing optimistically', async () => {
    const track = makeTrack()
    library.mockResolvedValue([track])

    // isPlaying=true so the optimistic state resolves to the "playing" class.
    const { container } = renderAlbumView([track], { isPlaying: true })
    const row = await rowForTrack(container, track.id)
    expect(row.className).not.toContain('Playing')

    fireEvent.click(row)

    await waitFor(() =>
      expect(
        container.querySelector<HTMLElement>(`li[data-track-id="${track.id}"]`)!.className,
      ).toContain('Playing'),
    )
  })

  it('surfaces an error when clear-and-play fails', async () => {
    const track = makeTrack()
    library.mockResolvedValue([track])
    clearAndPlay.mockRejectedValueOnce(new Error('mpd unreachable'))

    const { container, findByText } = renderAlbumView([track])
    fireEvent.click(await rowForTrack(container, track.id))

    expect(await findByText('mpd unreachable')).toBeTruthy()
  })
})
