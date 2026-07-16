import { useEffect, useRef, useState } from 'react'
import type { PlayerStatus, QueueResponse, StatusEvent } from './types'

export interface PlayerState {
  status: PlayerStatus | null
  queue: QueueResponse | null
  connected: boolean
  error: string | null
}

const MAX_BACKOFF = 10000

function wsUrl(): string {
  const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${proto}//${window.location.host}/api/ws`
}

/**
 * Single live connection to `/api/ws`. Seeds state from the first snapshot the
 * server sends on connect, then applies every pushed `StatusEvent`. Reconnects
 * with exponential backoff and re-seeds on reopen. Replaces the old
 * `setInterval` polls of `/api/status` + `/api/queue` (issue #3).
 */
export function usePlayerStatus(): PlayerState {
  const [state, setState] = useState<PlayerState>({
    status: null,
    queue: null,
    connected: false,
    error: null,
  })
  const backoff = useRef(1000)
  const stopped = useRef(false)

  useEffect(() => {
    stopped.current = false
    let ws: WebSocket | null = null
    let timer: ReturnType<typeof setTimeout> | null = null

    const connect = () => {
      if (stopped.current) return
      try {
        ws = new WebSocket(wsUrl())
      } catch {
        scheduleReconnect()
        return
      }

      ws.onopen = () => {
        backoff.current = 1000
        setState((s) => ({ ...s, connected: true, error: null }))
      }

      ws.onmessage = (ev) => {
        let event: StatusEvent
        try {
          event = JSON.parse(ev.data)
        } catch {
          return
        }
        if (event.type === 'status') {
          setState((s) => ({ ...s, status: event.Status }))
        } else if (event.type === 'queue') {
          setState((s) => ({ ...s, queue: event.Queue }))
        }
      }

      ws.onerror = () => {
        setState((s) => ({
          ...s,
          connected: false,
          error: 'Connection to player lost — retrying…',
        }))
      }

      ws.onclose = () => {
        setState((s) => ({ ...s, connected: false }))
        scheduleReconnect()
      }
    }

    const scheduleReconnect = () => {
      if (stopped.current) return
      const delay = backoff.current
      backoff.current = Math.min(backoff.current * 2, MAX_BACKOFF)
      timer = setTimeout(connect, delay)
    }

    connect()

    return () => {
      stopped.current = true
      if (timer) clearTimeout(timer)
      if (ws) {
        ws.onclose = null
        ws.close()
      }
    }
  }, [])

  return state
}
