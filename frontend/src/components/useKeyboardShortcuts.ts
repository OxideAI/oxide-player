import { useEffect, useRef } from 'react'
import type { PlayerStatus } from '../types'
import { actionForEvent, SEEK_STEP, VOLUME_STEP, type ActionId } from './shortcuts'

function isTypingTarget(el: EventTarget | null): boolean {
  if (!(el instanceof HTMLElement)) return false
  const tag = el.tagName
  return tag === 'INPUT' || tag === 'TEXTAREA' || el.isContentEditable
}

export interface ShortcutHandlers {
  status: PlayerStatus | null
  onTogglePlay: () => void
  onNext: () => void
  onPrev: () => void
  onSeek: (seconds: number) => void
  onVolume: (v: number) => void
  onToggleKiosk: () => void
  onToggleShuffle: () => void
  onToggleMute: () => void
  onHelp: () => void
  onFeedback: (text: string) => void
}

export function useKeyboardShortcuts(handlers: ShortcutHandlers): void {
  const ref = useRef(handlers)
  ref.current = handlers

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return
      const h = ref.current
      const action = actionForEvent(e)
      if (!action) return
      // Shortcuts are disabled while typing in an input, textarea, or
      // contenteditable element (search box, DSP settings, album rename, etc.).
      // Help is also blocked from inputs to avoid hijacking typing.
      if (isTypingTarget(e.target)) return

      const status = h.status
      const run = (fn: () => void) => {
        e.preventDefault()
        fn()
      }

      const fire = (id: ActionId, fn: () => void, feedback?: string) => {
        if (action !== id) return
        run(fn)
        if (feedback) h.onFeedback(feedback)
      }

      switch (action) {
        case 'togglePlay':
          fire('togglePlay', h.onTogglePlay, status?.state === 'playing' ? '⏸ Paused' : '▶ Playing')
          return
        case 'volumeUp':
        case 'volumeDown': {
          const cur = status?.volume ?? 0
          const v = Math.max(0, Math.min(100, cur + (action === 'volumeUp' ? VOLUME_STEP : -VOLUME_STEP)))
          fire(action, () => h.onVolume(v), `Volume ${v}%`)
          return
        }
        case 'next':
          fire('next', h.onNext, '⏭ Next')
          return
        case 'prev':
          fire('prev', h.onPrev, '⏮ Previous')
          return
        case 'seekBack':
        case 'seekFwd': {
          const cur = status?.elapsed ?? 0
          const t = Math.max(0, cur + (action === 'seekFwd' ? SEEK_STEP : -SEEK_STEP))
          fire(action, () => h.onSeek(t), `Seek ${t.toFixed(0)}s`)
          return
        }
        case 'toggleKiosk':
          fire('toggleKiosk', h.onToggleKiosk, 'Kiosk mode')
          return
        case 'toggleShuffle':
          fire('toggleShuffle', h.onToggleShuffle, status?.random ? 'Shuffle off' : 'Shuffle on')
          return
        case 'toggleMute':
          fire('toggleMute', h.onToggleMute, status?.volume === 0 ? 'Unmuted' : 'Muted')
          return
        case 'help':
          fire('help', h.onHelp, undefined)
          return
      }
    }

    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])
}
