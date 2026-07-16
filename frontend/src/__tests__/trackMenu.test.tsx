import { describe, it, expect } from 'vitest'
import { render } from '@testing-library/react'
import { TrackMenu } from '../components/TrackMenu'
import { Track } from '../types'

function makeTrack(): Track {
  return {
    id: 7,
    uri: 'Diana Krall - The Girl In The Other Room/01 Diana Krall - Stop This World.m4a',
    path: 'Diana Krall - The Girl In The Other Room/01 Diana Krall - Stop This World.m4a',
    title: 'Stop This World',
    artist: 'Diana Krall',
    album: 'The Girl In The Other Room',
    has_cover: true,
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
  }
}

const track = makeTrack()

function wrapperClass(playing: boolean): string {
  const { container } = render(<TrackMenu tracks={[track]} playing={playing} />)
  const wrap = container.firstChild as HTMLElement
  return wrap.className
}

describe('TrackMenu playing indicator (issue #37)', () => {
  it('applies the .playing class when playing is true', () => {
    const cls = wrapperClass(true)
    expect(cls).toContain('playing')
  })

  it('omits the .playing class when playing is false', () => {
    const cls = wrapperClass(false)
    expect(cls).not.toContain('playing')
  })

  it('renders the 3-dot menu button', () => {
    const { container } = render(<TrackMenu tracks={[track]} />)
    const btn = container.querySelector('button[aria-label="More actions"]')
    expect(btn).not.toBeNull()
  })
})
