import { describe, it, expect, beforeEach } from 'vitest'
import { parsePath, buildPath } from '../App'

// Bug-rule (ui-navigation skill): any change to Route parsing/building must
// keep the back button (popstate) and deep links working — covered here.

describe('App routing', () => {
  beforeEach(() => {
    window.history.pushState({}, '', '/')
  })

  it('parses /radio into the radio tab', () => {
    window.history.pushState({}, '', '/radio')
    expect(parsePath()).toEqual({ tab: 'radio', album: null })
  })

  it('keeps album deep links working', () => {
    window.history.pushState({}, '', '/library/Artist%20%2F%20Album')
    expect(parsePath()).toEqual({ tab: 'library', album: 'Artist / Album' })
  })

  it('builds /radio path', () => {
    expect(buildPath({ tab: 'radio', album: null })).toBe('/radio')
  })

  it('builds album path with encoding', () => {
    expect(buildPath({ tab: 'library', album: 'A/B' })).toBe('/library/A%2FB')
  })

  it('falls back to library for unknown paths', () => {
    window.history.pushState({}, '', '/nope')
    expect(parsePath()).toEqual({ tab: 'library', album: null })
  })
})
