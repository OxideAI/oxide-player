import { useCallback, useEffect, useRef, useState } from 'react'
import type { PlayerStatus } from '../types'

const clamp = (v: number, lo: number, hi: number) => (v < lo ? lo : v > hi ? hi : v)

/**
 * Drive the playback cursor locally between the 1s server polls so the
 * progress bar sweeps continuously instead of stepping once per second.
 * While playing it advances `elapsed` by wall-clock time since the last
 * server sample; each poll re-anchors it to the authoritative value.
 */
export function useSmoothElapsed(status: PlayerStatus | null, duration: number): { elapsed: number; reset: (v: number) => void } {
  const [elapsed, setElapsed] = useState(status?.elapsed ?? 0)
  const baseRef = useRef({ e: status?.elapsed ?? 0, t: performance.now() })

  const reset = useCallback((v: number) => {
    baseRef.current = { e: v, t: performance.now() }
    setElapsed(v)
  }, [])
  useEffect(() => {
    const e = status?.elapsed ?? 0
    baseRef.current = { e, t: performance.now() }
    setElapsed(e)
  }, [status?.elapsed, status?.current_song?.id])

  const playing = status?.state === 'playing'
  useEffect(() => {
    if (!playing) return
    let raf = 0
    const tick = () => {
      const { e: base, t } = baseRef.current
      const next = base + (performance.now() - t) / 1000
      setElapsed(duration > 0 && next > duration ? duration : next)
      raf = requestAnimationFrame(tick)
    }
    raf = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(raf)
  }, [playing, duration])

  return { elapsed, reset }
}

/**
 * Optimistic, throttled drag control. The visible value follows pointer
 * input immediately and writes to the backend coalesced (at most one commit
 * per `throttleMs`, plus a trailing commit on release). While actively
 * dragged, the server's value is ignored so the thumb never fights the user.
 */
export function useDragValue(
  value: number,
  commit: (v: number) => void,
  throttleMs = 120,
) {
  const [local, setLocal] = useState(value)
  const draggingRef = useRef(false)
  const pendingRef = useRef<number | null>(null)
  const lastRef = useRef(0)

  useEffect(() => {
    if (!draggingRef.current) setLocal(value)
  }, [value])

  const begin = useCallback(() => {
    draggingRef.current = true
    pendingRef.current = null
  }, [])

  const move = useCallback(
    (v: number) => {
      setLocal(v)
      const now = performance.now()
      if (now - lastRef.current >= throttleMs) {
        lastRef.current = now
        commit(v)
        pendingRef.current = null
      } else {
        pendingRef.current = v
      }
    },
    [commit, throttleMs],
  )

  const end = useCallback(() => {
    draggingRef.current = false
    const pending = pendingRef.current
    pendingRef.current = null
    lastRef.current = 0
    if (pending !== null) commit(pending)
  }, [commit])

  const isDragging = useCallback(() => draggingRef.current, [])

  return { local, begin, move, end, isDragging }
}

/** Map a pointer X position within `el` to a 0..1 fraction of the track. */
export function fracFromPointer(el: HTMLElement, clientX: number): number {
  const rect = el.getBoundingClientRect()
  return clamp((clientX - rect.left) / rect.width, 0, 1)
}
