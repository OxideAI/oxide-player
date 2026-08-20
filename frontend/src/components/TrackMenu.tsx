import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { Track } from "../types";
import { api, toPlayRef } from "../api";
import { displayTitle } from "../util";
import { FileInfo } from "./FileInfo";
import styles from "./TrackMenu.module.css";

interface Props {
  tracks: Track[];
  label?: string;
  playing?: boolean;
  onPlayNext?: () => void;
  onClearAndPlay?: () => void;
  onAdded?: (msg: string) => void;
  onError?: (msg: string) => void;
}

function Dots() {
  return (
    <svg
      viewBox="0 0 24 24"
      width="17"
      height="17"
      fill="currentColor"
      aria-hidden
    >
      <circle cx="6" cy="12" r="1.7" />
      <circle cx="12" cy="12" r="1.7" />
      <circle cx="18" cy="12" r="1.7" />
    </svg>
  );
}

function MI({ d }: { d: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      width="16"
      height="16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d={d} />
    </svg>
  );
}

export function TrackMenu({
  tracks,
  label,
  playing,
  onPlayNext,
  onClearAndPlay,
  onAdded,
  onError,
}: Props) {
  const [open, setOpen] = useState(false);
  const [showPlaylists, setShowPlaylists] = useState(false);
  const [playlists, setPlaylists] = useState<string[]>([]);
  const [infoTrack, setInfoTrack] = useState<Track | null>(null);
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node))
        setOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [open]);

  const run = async (fn: () => Promise<unknown>, ok: string) => {
    try {
      await fn();
      onAdded?.(ok);
    } catch (e) {
      onError?.(e instanceof Error ? e.message : String(e));
    }
  };

  const openPlaylists = async () => {
    try {
      setPlaylists(await api.playlists());
    } catch (e) {
      onError?.(e instanceof Error ? e.message : String(e));
    }
    setShowPlaylists(true);
  };

  const addTo = (name: string) => {
    setShowPlaylists(false);
    setOpen(false);
    void run(
      () => api.addToPlaylist(name, tracks.map(toPlayRef)),
      `Added ${tracks.length} to “${name}”`,
    );
  };

  const single = tracks.length === 1 ? tracks[0] : null;

  return (
    <div
      className={`${styles.trackMenu}${playing ? ` ${styles.playing}` : ""}`}
      ref={wrapRef}
    >
      <button
        className={styles.trackMenuBtn}
        title={label ?? "More actions"}
        aria-label={label ?? "More actions"}
        onClick={(e) => {
          e.stopPropagation();
          setOpen((v) => !v);
        }}
      >
        <Dots />
      </button>
      {open && (
        <div
          className={styles.trackMenuPop}
          onClick={(e) => e.stopPropagation()}
        >
          <button
            className={styles.trackMenuItem}
            onClick={() => {
              setOpen(false);
              if (onPlayNext) onPlayNext();
              else
                void run(
                  () => api.playNext(tracks.map(toPlayRef)),
                  `Queued “${single ? displayTitle(single) : `${tracks.length} tracks`}” next`,
                );
            }}
          >
            <MI d="M5 5v14l11-7-11-7zM18 5v14" />
            Play next
          </button>
          <button
            className={styles.trackMenuItem}
            onClick={() => {
              setOpen(false);
              if (onClearAndPlay) onClearAndPlay();
              else
                void run(
                  () => api.clearAndPlay(tracks.map(toPlayRef)),
                  `Playing ${single ? displayTitle(single) : `${tracks.length} tracks`}`,
                );
            }}
          >
            <MI d="M5 5v14l11-7-11-7zM19 5a2 2 0 1 0 0 0" />
            Clear and play
          </button>
          <button
            className={styles.trackMenuItem}
            onClick={() => void openPlaylists()}
          >
            <MI d="M12 5v14M5 12h14" />
            Add to playlist…
          </button>
          {single && (
            <button
              className={styles.trackMenuItem}
              onClick={() => setInfoTrack(single)}
            >
              <MI d="M12 8v.01M11 11h1v5h1" />
              File info
            </button>
          )}
        </div>
      )}
      {showPlaylists &&
        createPortal(
          <div
            className={styles.trackMenuModal}
            onClick={() => setShowPlaylists(false)}
          >
            <div
              className={styles.trackMenuModalBox}
              onClick={(e) => e.stopPropagation()}
            >
              <div className={styles.trackMenuModalTitle}>Add to playlist</div>
              <ul className={styles.trackMenuPlaylistList}>
                {playlists.length === 0 && (
                  <li className={styles.trackMenuEmpty}>No playlists yet</li>
                )}
                {playlists.map((p) => (
                  <li key={p}>
                    <button
                      className={styles.trackMenuItem}
                      onClick={() => addTo(p)}
                    >
                      {p}
                    </button>
                  </li>
                ))}
              </ul>
              <button
                className={styles.trackMenuClose}
                onClick={() => setShowPlaylists(false)}
              >
                Close
              </button>
            </div>
          </div>,
          document.body,
        )}
      {infoTrack &&
        createPortal(
          <FileInfo track={infoTrack} onClose={() => setInfoTrack(null)} />,
          document.body,
        )}
    </div>
  );
}
