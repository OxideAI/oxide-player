import type { Track } from './types'

const DB_NAME = 'oxide-player'
const DB_VERSION = 1
const STORE_NAME = 'snapshots'
const LIBRARY_KEY = 'library-v1'
const LOCAL_KEY = 'oxide:library:v1'

interface Snapshot {
  tracks: Track[]
}

function localRead(): Track[] | null {
  try {
    const raw = localStorage.getItem(LOCAL_KEY)
    if (!raw) return null
    const snapshot = JSON.parse(raw) as Snapshot
    return Array.isArray(snapshot.tracks) ? snapshot.tracks : null
  } catch {
    return null
  }
}

function localWrite(tracks: Track[]): void {
  try {
    localStorage.setItem(LOCAL_KEY, JSON.stringify({ tracks }))
  } catch {
    // Storage is optional: the network remains the source of truth.
  }
}

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION)
    request.onupgradeneeded = () => {
      request.result.createObjectStore(STORE_NAME)
    }
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error ?? new Error('Unable to open library cache'))
  })
}

export async function readLibraryCache(): Promise<Track[] | null> {
  if (typeof indexedDB === 'undefined') return localRead()
  try {
    const db = await openDb()
    const snapshot = await new Promise<Snapshot | undefined>((resolve, reject) => {
      const request = db.transaction(STORE_NAME, 'readonly').objectStore(STORE_NAME).get(LIBRARY_KEY)
      request.onsuccess = () => resolve(request.result as Snapshot | undefined)
      request.onerror = () => reject(request.error)
    })
    db.close()
    return snapshot && Array.isArray(snapshot.tracks) ? snapshot.tracks : null
  } catch {
    return localRead()
  }
}

export async function writeLibraryCache(tracks: Track[]): Promise<void> {
  if (typeof indexedDB === 'undefined') {
    localWrite(tracks)
    return
  }
  try {
    const db = await openDb()
    await new Promise<void>((resolve, reject) => {
      const request = db
        .transaction(STORE_NAME, 'readwrite')
        .objectStore(STORE_NAME)
        .put({ tracks } satisfies Snapshot, LIBRARY_KEY)
      request.onsuccess = () => resolve()
      request.onerror = () => reject(request.error)
    })
    db.close()
  } catch {
    localWrite(tracks)
  }
}
