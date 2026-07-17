/// The wire envelope sent to the playback endpoints (play-next, clear-play,
/// playlist add). Maps a library `Track` to the shape the backend `TrackRef`
/// deserializer expects: `{ uri, start, end, track_id }`.
export interface PlayRef {
  uri: string
  start: number | undefined
  end: number | undefined
  track_id: number
}

export function toPlayRef(t: {
  uri: string
  start_time?: number | null
  end_time?: number | null
  id: number
}): PlayRef {
  return {
    uri: t.uri,
    start: t.start_time ?? undefined,
    end: t.end_time ?? undefined,
    track_id: t.id,
  }
}

export function fmtTime(s: number | null | undefined): string {
  if (s == null || !isFinite(s) || s < 0) s = 0
  const m = Math.floor(s / 60)
  const sec = Math.floor(s % 60)
  return `${m}:${sec.toString().padStart(2, '0')}`
}

export function displayTitle(t: { title: string | null; uri: string }): string {
  if (t.title) return t.title
  const name = t.uri.split('/').pop() || t.uri
  return name.replace(/\.[^./\\]+$/, '')
}

export function audioQuality(t: {
  format?: string | null
  sample_rate?: number | null
  bit_depth?: number | null
  channels?: number | null
}): string {
  const parts: string[] = []
  if (t.sample_rate) {
    const k = t.sample_rate / 1000
    parts.push(Number.isInteger(k) ? `${k} kHz` : `${k.toFixed(1)} kHz`)
  }
  if (t.bit_depth) parts.push(`${t.bit_depth}-bit`)
  if (t.channels) parts.push(`${t.channels}ch`)
  const q = parts.join(' / ')
  return t.format ? `${t.format.toUpperCase()} · ${q}` : q
}
