import type { Track } from './types'

const DB_NAME = 'oxide-player'
const DB_VERSION = 1
const STORE_NAME = 'snapshots'
const LIBRARY_KEY = 'library-v1'
const LOCAL_KEY = 'oxide:library:v1'

export interface LibraryCacheSnapshot {
  tracks: Track[]
  etag: string | null
}

interface StoredSnapshot {
  tracks: Track[]
  etag?: string | null
}

function normalize(snapshot: StoredSnapshot | null | undefined): LibraryCacheSnapshot | null {
  return snapshot && Array.isArray(snapshot.tracks)
    ? { tracks: snapshot.tracks, etag: snapshot.etag ?? null }
    : null
}

function localRead(): LibraryCacheSnapshot | null {
  try {
    const raw = localStorage.getItem(LOCAL_KEY)
    return raw ? normalize(JSON.parse(raw) as StoredSnapshot) : null
  } catch {
    return null
  }
}

function localWrite(snapshot: LibraryCacheSnapshot): void {
  try {
    localStorage.setItem(LOCAL_KEY, JSON.stringify(snapshot))
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

export async function readLibraryCache(): Promise<LibraryCacheSnapshot | null> {
  if (typeof indexedDB === 'undefined') return localRead()
  try {
    const db = await openDb()
    const stored = await new Promise<StoredSnapshot | undefined>((resolve, reject) => {
      const request = db.transaction(STORE_NAME, 'readonly').objectStore(STORE_NAME).get(LIBRARY_KEY)
      request.onsuccess = () => resolve(request.result as StoredSnapshot | undefined)
      request.onerror = () => reject(request.error)
    })
    db.close()
    return normalize(stored)
  } catch {
    return localRead()
  }
}

export async function writeLibraryCache(tracks: Track[], etag: string | null): Promise<void> {
  const snapshot: LibraryCacheSnapshot = { tracks, etag }
  if (typeof indexedDB === 'undefined') {
    localWrite(snapshot)
    return
  }
  try {
    const db = await openDb()
    await new Promise<void>((resolve, reject) => {
      const request = db
        .transaction(STORE_NAME, 'readwrite')
        .objectStore(STORE_NAME)
        .put(snapshot satisfies StoredSnapshot, LIBRARY_KEY)
      request.onsuccess = () => resolve()
      request.onerror = () => reject(request.error)
    })
    db.close()
  } catch {
    localWrite(snapshot)
  }
}

export async function removeTrackFromLibraryCache(trackId: number): Promise<void> {
  const snapshot = await readLibraryCache()
  if (snapshot === null || !snapshot.tracks.some((track) => track.id === trackId)) return
  await writeLibraryCache(
    snapshot.tracks.filter((track) => track.id !== trackId),
    snapshot.etag,
  )
}
