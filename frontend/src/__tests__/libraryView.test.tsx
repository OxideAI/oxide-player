import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { render, cleanup, waitFor, fireEvent } from '@testing-library/react'
import { Track } from '../types'

// Mock the api module so the component's library load and playback calls are
// observable. Only the methods LibraryView actually touches need real spies.
const clearAndPlay = vi.fn<(tracks: unknown) => Promise<unknown>>(() => Promise.resolve({}))
const play = vi.fn<(...args: unknown[]) => Promise<unknown>>(() => Promise.resolve({}))
const library = vi.fn<() => Promise<Track[]>>()

vi.mock('../api', () => ({
  api: {
    library: () => library(),
    clearAndPlay: (tracks: unknown) => clearAndPlay(tracks),
    play: (...args: unknown[]) => play(...args),
    coverUrl: (key: string) => `/cover/${key}`,
  },
}))

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

function renderAlbumView(track: Track) {
  // Passing `album` equal to the track's folder key opens the album view
  // directly, so the track row is rendered without an extra folder click.
  const folderKey = track.uri.slice(0, track.uri.lastIndexOf('/'))
  return render(
    <LibraryView
      refreshToken={0}
      onRefresh={noop}
      onRescanArt={noop}
      nowPlayingUri={null}
      nowPlayingId={null}
      isPlaying={false}
      album={folderKey}
      onAlbumChange={() => {}}
    />,
  )
}

describe('LibraryView track click (issue #32)', () => {
  beforeEach(() => {
    clearAndPlay.mockClear()
    play.mockClear()
    // Reveal (used by the album view) needs IntersectionObserver, absent in jsdom.
    // @ts-expect-error jsdom lacks IntersectionObserver
    globalThis.IntersectionObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
  })

  afterEach(() => cleanup())

  it('clears the queue and plays the clicked track (not append via api.play)', async () => {
    const track = makeTrack()
    library.mockResolvedValue([track])

    const { findByText } = renderAlbumView(track)

    const title = await findByText('Stop This World')
    const row = title.closest('li')
    expect(row).not.toBeNull()
    fireEvent.click(row!)

    await waitFor(() => expect(clearAndPlay).toHaveBeenCalledTimes(1))
    // Regression guard for #32: must NOT use the append-to-queue api.play path.
    expect(play).not.toHaveBeenCalled()
  })

  it('sends the single-track play envelope { uri, start, end, track_id }', async () => {
    const track = makeTrack({ id: 42, start_time: 12.5, end_time: 200 })
    library.mockResolvedValue([track])

    const { findByText } = renderAlbumView(track)

    const title = await findByText('Stop This World')
    fireEvent.click(title.closest('li')!)

    await waitFor(() => expect(clearAndPlay).toHaveBeenCalledTimes(1))
    expect(clearAndPlay).toHaveBeenCalledWith([
      { uri: track.uri, start: 12.5, end: 200, track_id: 42 },
    ])
  })

  it('maps null cue times to undefined in the play envelope', async () => {
    const track = makeTrack({ id: 3, start_time: null, end_time: null })
    library.mockResolvedValue([track])

    const { findByText } = renderAlbumView(track)

    const title = await findByText('Stop This World')
    fireEvent.click(title.closest('li')!)

    await waitFor(() => expect(clearAndPlay).toHaveBeenCalledTimes(1))
    expect(clearAndPlay).toHaveBeenCalledWith([
      { uri: track.uri, start: undefined, end: undefined, track_id: 3 },
    ])
  })
})
