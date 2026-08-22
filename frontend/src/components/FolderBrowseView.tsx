import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Track } from "../types";
import { api, toPlayRef } from "../api";
import { displayTitle, fmtTime } from "../util";
import {
  ancestors,
  buildFolderTree,
  descendantTracks,
  folderBasename,
  folderParent,
  sortedSubfolders,
} from "../folderTree";
import { TrackMenu } from "./TrackMenu";
import styles from "./FolderBrowseView.module.css";
import libStyles from "./LibraryView.module.css";

/**
 * A folder's thumbnail. Prefers the live per-directory cover (`folder.jpg` /
 * `cover.jpg`, read straight from disk by the backend so art added after a
 * scan still shows), falls back to the first descendant album cover (the only
 * art that existed before per-directory covers), then to the ♫ placeholder.
 */
function FolderThumb({ dir, coverKey }: { dir: string; coverKey: string | null }) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    setSrc(api.coverForDir(dir));
  }, [dir]);
  if (!src) return <span className={styles.folderPh}>♫</span>;
  return (
    <img
      src={src}
      alt=""
      loading="lazy"
      decoding="async"
      onError={() => {
        if (coverKey) setSrc(api.coverUrl(coverKey));
        else setSrc(null);
      }}
    />
  );
}

interface Props {
  tracks: Track[];
  loading: boolean;
  refreshing: boolean;
  error: string | null;
  setError: (e: string | null) => void;
  nowPlayingId: number | null;
  isPlaying: boolean;
  folderPath: string | null;
  onFolderChange: (p: string | null) => void;
  libraryView: "albums" | "folders";
  onViewChange: (m: "albums" | "folders") => void;
  onRefresh: () => Promise<void>;
  onRescanArt: () => Promise<void>;
}

export function FolderBrowseView({
  tracks,
  loading,
  refreshing,
  error,
  setError,
  nowPlayingId,
  isPlaying,
  folderPath,
  onFolderChange,
  libraryView,
  onViewChange,
  onRefresh,
  onRescanArt,
}: Props) {
  const [query, setQuery] = useState("");
  const [playingUri, setPlayingUri] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const toastTimer = useRef<number | undefined>(undefined);
  const notify = useCallback((msg: string) => {
    setToast(msg);
    if (toastTimer.current !== undefined) window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 2500);
  }, []);
  useEffect(
    () => () => {
      if (toastTimer.current !== undefined) window.clearTimeout(toastTimer.current);
    },
    [],
  );

  const tree = useMemo(() => buildFolderTree(tracks), [tracks]);

  const normalizedQuery = query.trim().toLowerCase();

  // Validate current folderPath: if not in tree and not null, reset to root
  useEffect(() => {
    if (folderPath !== null && !tree.nodes.has(folderPath) && tree.nodes.size > 0) {
      // Maybe stale path after rescan
      onFolderChange(null);
    }
  }, [folderPath, tree, onFolderChange]);

  const directSubfolders = useMemo(() => sortedSubfolders(folderPath, tree), [folderPath, tree]);
  const directTracks = useMemo(() => {
    if (folderPath === null) return [] as Track[];
    return [...(tree.directTracks.get(folderPath) ?? [])];
  }, [folderPath, tree]);

  const filteredSubfolders = useMemo(() => {
    if (!normalizedQuery) return directSubfolders;
    return directSubfolders.filter((d) => {
      const base = folderBasename(d).toLowerCase();
      return base.includes(normalizedQuery);
    });
  }, [directSubfolders, normalizedQuery]);

  const filteredDirectTracks = useMemo(() => {
    if (!normalizedQuery) return directTracks;
    return directTracks.filter((t) =>
      [t.title, t.artist].filter(Boolean).some((v) => v!.toLowerCase().includes(normalizedQuery)),
    );
  }, [directTracks, normalizedQuery]);

  const nowId = useMemo(
    () =>
      nowPlayingId ??
      (playingUri ? (tracks.find((t) => t.uri === playingUri)?.id ?? null) : null),
    [nowPlayingId, playingUri, tracks],
  );

  const play = async (t: Track) => {
    setPlayingUri(t.uri);
    try {
      await api.clearAndPlay([toPlayRef(t)]);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const rescanArt = async () => {
    setError(null);
    try {
      await onRescanArt();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const breadcrumbParts = folderPath ? ancestors(folderPath) : [];
  const currentNode = folderPath ? tree.nodes.get(folderPath) : null;
  const descendantCount = useMemo(() => {
    if (folderPath === null) return tree.directTracks.size ? [...tree.directTracks.values()].reduce((a, v) => a + v.length, 0) : 0;
    return currentNode?.trackCount ?? 0;
  }, [folderPath, currentNode, tree]);
  const currentDescendants = useMemo(() => descendantTracks(folderPath, tree), [folderPath, tree]);

  const parentPath = folderPath ? folderParent(folderPath) : null;
  // When at child of root, parent might not be in nodes — allow going to null root
  const canGoUp = folderPath !== null;

  const busy = loading || refreshing;
  const coverKeyForDir = (dir: string) => tree.nodes.get(dir)?.coverKey ?? null;

  return (
    <div className={styles.wrap} aria-busy={busy}>
      {toast && <div className={libStyles.toast}>{toast}</div>}

      <div className={libStyles.toolbar}>
        <div className={libStyles.viewSwitch} role="tablist" aria-label="Library view">
          <button
            role="tab"
            aria-selected={libraryView !== "folders"}
            className={libraryView !== "folders" ? libStyles.viewActive : libStyles.viewIdle}
            onClick={() => onViewChange("albums")}
          >
            Albums
          </button>
          <button
            role="tab"
            aria-selected={libraryView === "folders"}
            className={libraryView === "folders" ? libStyles.viewActive : libStyles.viewIdle}
            onClick={() => onViewChange("folders")}
          >
            Folders
          </button>
        </div>
        {canGoUp && (
          <button
            className={libStyles.ghost}
            onClick={() => {
              if (parentPath !== null && tree.nodes.has(parentPath)) onFolderChange(parentPath);
              else onFolderChange(null);
            }}
          >
            <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
              <path d="M14 6l-6 6 6 6" />
            </svg>
            Back
          </button>
        )}
        <div className={libStyles.searchShell}>
          <svg className={libStyles.searchIcon} viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round">
            <circle cx="11" cy="11" r="6.5" />
            <path d="M20 20l-3.6-3.6" />
          </svg>
          <input
            className={libStyles.search}
            placeholder={folderPath ? "Search this folder…" : "Search folders…"}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <button className={libStyles.pill} onClick={onRefresh}>
          Refresh
        </button>
        <button className={libStyles.pill} onClick={rescanArt}>
          Rescan art
        </button>
        <span className={libStyles.count}>
          {loading && tracks.length === 0 ? "loading…" : `${tree.nodes.size} folders · ${tracks.length} tracks`}
        </span>
      </div>

      {error && (
        <div className={libStyles.error} role={tracks.length > 0 ? "status" : "alert"}>
          {error}
        </div>
      )}

      {!loading && !error && tracks.length === 0 && (
        <div className={libStyles.empty}>
          <div className={libStyles.emptyMark}>♪</div>
          <p>The library is empty.</p>
          <button className={libStyles.pill} onClick={onRefresh}>
            Scan music folder
          </button>
        </div>
      )}

      {tracks.length > 0 && (
        <>
          {/* Breadcrumb */}
          <nav className={styles.breadcrumb} aria-label="Folder path">
            <button className={breadcrumbParts.length === 0 ? styles.crumbActive : styles.crumb} onClick={() => onFolderChange(null)}>
              Library
            </button>
            {breadcrumbParts.map((part, i) => {
              const isLast = i === breadcrumbParts.length - 1;
              return (
                <span key={part} className={styles.crumbSepWrap}>
                  <span className={styles.sep}>/</span>
                  <button
                    className={isLast ? styles.crumbActive : styles.crumb}
                    onClick={() => onFolderChange(part)}
                    title={part}
                  >
                    {folderBasename(part) || part}
                  </button>
                </span>
              );
            })}
          </nav>

          {/* Current folder header with bulk actions */}
          {folderPath !== null && currentNode && (
            <div className={styles.currentHead}>
              <div className={styles.currentTitle}>{currentNode.basename}</div>
              <div className={styles.currentMeta}>
                {descendantCount} tracks
                {currentNode.childCount > 0 ? ` · ${currentNode.childCount} subfolders` : ""}
              </div>
              <div className={styles.currentActions}>
                <TrackMenu
                  tracks={currentDescendants}
                  label="Folder actions"
                  onAdded={notify}
                  onError={setError}
                />
              </div>
            </div>
          )}

          {/* Subfolders */}
          {filteredSubfolders.length > 0 && (
            <ul className={styles.folderList} aria-label="Folders">
              {filteredSubfolders.map((dir) => {
                const node = tree.nodes.get(dir)!;
                const ck = coverKeyForDir(dir);
                const des = descendantTracks(dir, tree);
                return (
                  <li key={dir} className={styles.folderRow} onClick={() => onFolderChange(dir)}>
                    <span className={styles.folderThumb}>
                      <FolderThumb dir={dir} coverKey={ck} />
                    </span>
                    <span className={styles.folderName} title={dir}>
                      {node.basename}
                    </span>
                    <span className={styles.folderCount}>
                      {node.trackCount} tracks{node.childCount > 0 ? ` · ${node.childCount} folders` : ""}
                    </span>
                    <span
                      className={styles.folderMenu}
                      onClick={(e) => e.stopPropagation()}
                    >
                      <TrackMenu tracks={des} label={`Actions for ${node.basename}`} onAdded={notify} onError={setError} />
                    </span>
                  </li>
                );
              })}
            </ul>
          )}

          {/* Tracks directly in this folder */}
          {filteredDirectTracks.length > 0 && (
            <ul className={libStyles.list} aria-label="Tracks">
              {filteredDirectTracks.map((t) => (
                <li
                  key={t.id}
                  data-track-id={t.id}
                  className={nowId === t.id ? (isPlaying ? libStyles.rowPlaying : libStyles.rowPaused) : libStyles.row}
                  onClick={() => play(t)}
                >
                  <span className={libStyles.tPos}>
                    {nowId === t.id ? (
                      <span className={libStyles.rowEq} aria-hidden>
                        <i />
                        <i />
                        <i />
                      </span>
                    ) : (
                      (t.track ?? "") || "·"
                    )}
                  </span>
                  <span className={libStyles.tTitle}>{displayTitle(t)}</span>
                  <span className={libStyles.tArtist}>{t.artist ?? "—"}</span>
                  <span className={libStyles.tTime}>{fmtTime(t.duration)}</span>
                  <span className={libStyles.rowMenu}>
                    <TrackMenu tracks={[t]} playing={nowId === t.id && isPlaying} onAdded={notify} onError={setError} />
                  </span>
                </li>
              ))}
            </ul>
          )}

          {folderPath !== null && filteredSubfolders.length === 0 && filteredDirectTracks.length === 0 && !loading && (
            <div className={styles.emptyNote}>No tracks in this folder.</div>
          )}

          {folderPath === null && tree.roots.length === 0 && !loading && (
            <div className={styles.emptyNote}>Only directories containing scanned tracks appear. Empty or ignored folders are not shown.</div>
          )}
        </>
      )}
    </div>
  );
}
