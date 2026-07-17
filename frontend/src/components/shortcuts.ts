export const VOLUME_STEP = 5
export const SEEK_STEP = 5

export type ActionId =
  | 'togglePlay'
  | 'volumeUp'
  | 'volumeDown'
  | 'next'
  | 'prev'
  | 'seekBack'
  | 'seekFwd'
  | 'toggleKiosk'
  | 'toggleShuffle'
  | 'toggleMute'
  | 'help'

export interface Binding {
  id: ActionId
  key: string
  label: string
  desc: string
}

export const BINDINGS: Binding[] = [
  { id: 'togglePlay', key: ' ', label: 'Space', desc: 'Play / pause' },
  { id: 'volumeUp', key: 'ArrowUp', label: '↑', desc: 'Volume up' },
  { id: 'volumeDown', key: 'ArrowDown', label: '↓', desc: 'Volume down' },
  { id: 'prev', key: 'ArrowLeft', label: '←', desc: 'Previous track' },
  { id: 'next', key: 'ArrowRight', label: '→', desc: 'Next track' },
  { id: 'seekBack', key: ',', label: ',', desc: 'Seek −5s' },
  { id: 'seekFwd', key: '.', label: '.', desc: 'Seek +5s' },
  { id: 'toggleKiosk', key: 'k', label: 'K', desc: 'Toggle kiosk mode' },
  { id: 'toggleShuffle', key: 's', label: 'S', desc: 'Toggle shuffle' },
  { id: 'toggleMute', key: 'm', label: 'M', desc: 'Mute / unmute' },
  { id: 'help', key: '?', label: '?', desc: 'Show this help' },
  { id: 'help', key: 'h', label: 'H', desc: 'Show this help' },
]

const KEY_TO_ID = new Map<string, ActionId>()
for (const b of BINDINGS) KEY_TO_ID.set(b.key.toLowerCase(), b.id)

export function actionForEvent(e: KeyboardEvent): ActionId | null {
  return KEY_TO_ID.get(e.key.toLowerCase()) ?? null
}
