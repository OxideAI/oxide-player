import { useEffect, useId } from "react";
import type { Track } from "../types";
import { fmtTime } from "../util";
import styles from "./TrackMenu.module.css";

interface Props {
  name: string;
  tracks: Track[];
  onClose: () => void;
}

function Row({ label, value }: { label: string; value: string | null }) {
  if (!value) return null;
  return (
    <div className={styles.fileInfoRow}>
      <span className={styles.fileInfoLabel}>{label}</span>
      <span className={styles.fileInfoValue}>{value}</span>
    </div>
  );
}

function distinct(values: Array<string | number | null | undefined>): string[] {
  const result: string[] = [];
  const seen = new Set<string>();
  for (const value of values) {
    if (value === null || value === undefined) continue;
    const text = String(value).trim();
    if (text && !seen.has(text)) {
      seen.add(text);
      result.push(text);
    }
  }
  return result;
}

function list(values: Array<string | number | null | undefined>): string | null {
  const items = distinct(values);
  return items.length > 0 ? items.join(", ") : null;
}

function parentPath(path: string): string | null {
  const normalized = path.replace(/\\/g, "/").replace(/\/+$/, "");
  const separator = normalized.lastIndexOf("/");
  if (separator < 0) return null;
  if (separator === 0) return "/";
  return normalized.slice(0, separator);
}

function sampleRate(rate: number): string {
  const khz = rate / 1000;
  return `${Number.isInteger(khz) ? khz : khz.toFixed(1)} kHz`;
}

export function AlbumInfo({ name, tracks, onClose }: Props) {
  const titleId = useId();
  const albumArtists = list(tracks.map((track) => track.album_artist));
  const artists = distinct(tracks.map((track) => track.artist));
  const years = list(tracks.map((track) => track.year));
  const genres = list(tracks.map((track) => track.genre));
  const locations = list(tracks.map((track) => parentPath(track.path) ?? parentPath(track.uri)));
  const sources = list(tracks.map((track) => track.source));
  const formats = list(tracks.map((track) => track.format?.toUpperCase() ?? null));
  const sampleRates = list(
    distinct(tracks.map((track) => track.sample_rate)).map((rate) => sampleRate(Number(rate)))
  );
  const bitDepths = list(
    distinct(tracks.map((track) => track.bit_depth)).map((depth) => `${depth}-bit`)
  );
  const channels = list(tracks.map((track) => track.channels));
  const durations = tracks
    .map((track) => track.duration)
    .filter(
      (duration): duration is number =>
        duration !== null && Number.isFinite(duration) && duration >= 0
    );
  const duration =
    durations.length > 0 ? fmtTime(durations.reduce((total, value) => total + value, 0)) : null;
  const modified = tracks.reduce<number | null>((latest, track) => {
    if (!track.file_mtime) return latest;
    return latest === null ? track.file_mtime : Math.max(latest, track.file_mtime);
  }, null);
  const hasAudio =
    formats !== null || sampleRates !== null || bitDepths !== null || channels !== null;

  useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", closeOnEscape);
    return () => document.removeEventListener("keydown", closeOnEscape);
  }, [onClose]);

  return (
    <div className={styles.trackMenuModal} onClick={onClose}>
      <div
        className={styles.fileInfoBox}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onClick={(event) => event.stopPropagation()}
      >
        <div className={styles.fileInfoHead}>
          <span id={titleId} className={styles.fileInfoTitle}>
            {name}
          </span>
          <button className={styles.infoClose} onClick={onClose} aria-label="Close album info">
            ✕
          </button>
        </div>

        <div className={styles.fileInfoSection}>Album</div>
        <Row label="Album artist" value={albumArtists} />
        <Row label={artists.length === 1 ? "Artist" : "Artists"} value={artists.join(", ")} />
        <Row label="Year" value={years} />
        <Row label="Genre" value={genres} />
        <Row label="Tracks" value={String(tracks.length)} />
        <Row label="Duration" value={duration} />

        <div className={styles.fileInfoSection}>Location</div>
        <Row label="Full album path" value={locations} />
        <Row label="Library source" value={sources} />
        <Row
          label="Modified"
          value={modified === null ? null : new Date(modified * 1000).toLocaleString()}
        />

        {hasAudio ? (
          <>
            <div className={styles.fileInfoSection}>Audio</div>
            <Row label="Format" value={formats} />
            <Row label="Sample rate" value={sampleRates} />
            <Row label="Bit depth" value={bitDepths} />
            <Row label="Channels" value={channels} />
          </>
        ) : null}
      </div>
    </div>
  );
}
