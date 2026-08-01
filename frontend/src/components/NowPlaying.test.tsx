import { cleanup, render } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { PlayerStatus } from '../types'
import { NowPlaying } from './NowPlaying'

function status(volume: number | null): PlayerStatus {
  return {
    state: 'stopped',
    volume,
    current_song: null,
    elapsed: 0,
    duration: 0,
    outputs: [],
    error: null,
    random: false,
  }
}

describe('NowPlaying volume capability', () => {
  afterEach(() => cleanup())

  it('shows the volume slider when a switched output reports volume support', () => {
    const props = {
      queue: null,
      onTogglePlay: vi.fn(),
      onNext: vi.fn(),
      onPrev: vi.fn(),
      onSeek: vi.fn(),
      onVolume: vi.fn(),
      onOpenAlbum: vi.fn(),
    }
    const { queryByLabelText, getByLabelText, rerender } = render(
      <NowPlaying {...props} status={status(null)} />,
    )

    expect(queryByLabelText('Volume')).toBeNull()

    rerender(<NowPlaying {...props} status={status(69)} />)

    expect(getByLabelText('Volume')).toHaveProperty('value', '69')
  })
})
