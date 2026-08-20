import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import { readLibraryCache, writeLibraryCache } from "./libraryCache";
import type { Track } from "./types";

export interface UseLibraryTracksResult {
  tracks: Track[];
  loading: boolean;
  refreshing: boolean;
  error: string | null;
  setError: (e: string | null) => void;
  refresh: () => Promise<void>;
}

export function useLibraryTracks(refreshToken: number): UseLibraryTracksResult {
  const [tracks, setTracks] = useState<Track[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    setRefreshing(true);
    const cached = await readLibraryCache();
    if (cached !== null) {
      setTracks(cached.tracks);
      setLoading(false);
    }
    try {
      const latest =
        typeof api.librarySnapshot === "function"
          ? await api.librarySnapshot(cached?.etag ?? undefined)
          : {
              tracks: await api.library(),
              etag: null,
              notModified: false,
            };
      if (!latest.notModified && latest.tracks !== null) {
        setTracks(latest.tracks);
        void writeLibraryCache(latest.tracks, latest.etag);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load, refreshToken]);

  return { tracks, loading, refreshing, error, setError, refresh: load };
}
