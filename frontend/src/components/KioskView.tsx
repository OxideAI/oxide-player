import { useEffect, useState } from "react";
import type { PlayerStatus, QueueResponse } from "../types";
import { api } from "../api";
import { fmtTime, displayTitle, audioQuality, folderKey } from "../util";
import { useDragValue, useSmoothElapsed } from "./playerHooks";
import {
  Visualizer,
  DEFAULT_VIZ_PARAMS,
  VIZ_PRESETS,
  type VizParams,
  type VizStyle,
} from "./Visualizer";
import { VisualizerControls } from "./VisualizerControls";
import { useVisualizer } from "../useVisualizer";
import styles from "./KioskView.module.css";

interface Props {
  status: PlayerStatus | null;
  queue: QueueResponse | null;
  onTogglePlay: () => void;
  onNext: () => void;
  onPrev: () => void;
  onSeek: (seconds: number) => void;
  onVolume: (volume: number) => void;
  onOpenAlbum: (album: string) => void;
}

export function KioskView({
  status,
  onTogglePlay,
  onNext,
  onPrev,
  onSeek,
  onVolume,
  onOpenAlbum,
}: Props) {
  const loading = status === null;
  const song = status?.current_song ?? null;
  const cover = song?.has_cover
    ? api.coverUrl(song.cover_key ?? song.id)
    : null;
  const title = loading
    ? "Loading…"
    : song
      ? displayTitle(song)
      : "Nothing playing";
  const duration = status?.duration ?? 0;
  const playing = status?.state === "playing";

  const { elapsed: smoothElapsed, reset: resetElapsed } = useSmoothElapsed(
    status,
    duration,
  );
  const seek = useDragValue(smoothElapsed, onSeek);
  const vol = useDragValue(status?.volume ?? 0, onVolume);
  const volumeAvailable =
    status?.volume !== null && status?.volume !== undefined;

  // The websocket is cheap when FFT capture is disabled and keeping it
  // connected avoids a config-request race hiding a live analyzer.
  const frame = useVisualizer(true);

  // The selected look is persisted with the numeric tuning params on the
  // server, so kiosk mode keeps its configuration across restarts.
  const [vizParams, setVizParams] = useState<VizParams>(DEFAULT_VIZ_PARAMS);
  const [vizTuning, setVizTuning] = useState(false);
  useEffect(() => {
    let alive = true;
    api
      .getVizParams()
      .then((p: Record<string, number | string>) => {
        if (!alive) return;
        const style =
          typeof p.style === "string" && p.style in VIZ_PRESETS
            ? (p.style as VizStyle)
            : DEFAULT_VIZ_PARAMS.style;
        const preset = VIZ_PRESETS[style];
        setVizParams({
          ...preset,
          bloomAlpha:
            typeof p.bloom_alpha === "number"
              ? p.bloom_alpha
              : preset.bloomAlpha,
          bloomBeat:
            typeof p.bloom_beat === "number" ? p.bloom_beat : preset.bloomBeat,
          bloomEnergy:
            typeof p.bloom_energy === "number"
              ? p.bloom_energy
              : preset.bloomEnergy,
          bloomRadius:
            typeof p.bloom_radius === "number"
              ? p.bloom_radius
              : preset.bloomRadius,
          barIdle: typeof p.bar_idle === "number" ? p.bar_idle : preset.barIdle,
          barPeak: typeof p.bar_peak === "number" ? p.bar_peak : preset.barPeak,
          barGap: typeof p.bar_gap === "number" ? p.bar_gap : preset.barGap,
          barRadius:
            typeof p.bar_radius === "number" ? p.bar_radius : preset.barRadius,
          phaseSpeed:
            typeof p.phase_speed === "number"
              ? p.phase_speed
              : preset.phaseSpeed,
          blur: typeof p.blur === "number" ? p.blur : preset.blur,
        });
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);

  const seekFrac = duration > 0 ? Math.min(1, seek.local / duration) : 0;

  return (
    <div className={styles.kiosk}>
      <Visualizer playing={playing} frame={frame} params={vizParams} />
      <button
        className={styles.tune}
        onClick={() => setVizTuning((v) => !v)}
        title="Tune visualizer"
        aria-label="Tune visualizer"
      >
        <svg
          viewBox="0 0 24 24"
          width="16"
          height="16"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.7"
          strokeLinecap="round"
        >
          <path d="M4 8h10M18 8h2M4 16h2M10 16h10M14 6v4M8 14v4" />
        </svg>
      </button>
      {vizTuning && (
        <VisualizerControls
          params={vizParams}
          onChange={setVizParams}
          onClose={() => setVizTuning(false)}
        />
      )}
      <button
        className={styles.exit}
        onClick={() => {
          if (window.history.length > 1) window.history.back();
          else
            window.location.pathname =
              window.location.pathname.replace(/\/kiosk$/, "") || "/";
        }}
        title="Back"
        aria-label="Back"
      >
        <svg
          viewBox="0 0 24 24"
          width="18"
          height="18"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.7"
          strokeLinecap="round"
        >
          <path d="M6 6l12 12M18 6 6 18" />
        </svg>
      </button>

      <div className={styles.stage}>
        <div
          className={styles.art}
          style={cover ? { backgroundImage: `url(${cover})` } : undefined}
        >
          {!cover && <span className={styles.note} aria-hidden />}
          {cover && <span className={styles.artGlow} aria-hidden />}
        </div>

        <div className={styles.meta}>
          <span className={styles.eyebrow}>Now playing</span>
          <div className={styles.title}>{title}</div>
          {song && (
            <button
              type="button"
              className={styles.subBtn}
              onClick={() => onOpenAlbum(folderKey(song.uri))}
              title="Open album"
              aria-label={`Open album: ${[song.artist, song.album].filter(Boolean).join(" — ")}`}
            >
              {[song.artist, song.album].filter(Boolean).join(" · ")}
            </button>
          )}
          {song && <div className={styles.quality}>{audioQuality(song)}</div>}
        </div>

        <div className={styles.progress}>
          <span className={styles.time}>
            {fmtTime(seek.isDragging() ? seek.local : smoothElapsed)}
          </span>
          <input
            className={styles.bar}
            type="range"
            min={0}
            max={duration || 0}
            step="any"
            value={seek.local}
            style={{ ["--frac" as string]: seekFrac }}
            onPointerDown={seek.begin}
            onChange={(e) => seek.move(Number(e.target.value))}
            onPointerUp={(e) => {
              resetElapsed(Number((e.target as HTMLInputElement).value));
              seek.end();
            }}
            onPointerCancel={(e) => {
              resetElapsed(Number((e.target as HTMLInputElement).value));
              seek.end();
            }}
            aria-label="Seek"
          />
          <span className={styles.time}>{fmtTime(duration)}</span>
        </div>

        <div className={styles.controls}>
          <button className={styles.btn} onClick={onPrev} aria-label="previous">
            <svg
              viewBox="0 0 24 24"
              width="26"
              height="26"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="M18 6v12M8 12l8-6v12l-8-6z" />
            </svg>
          </button>
          <button
            className={`${styles.btn} ${styles.main}`}
            onClick={onTogglePlay}
            aria-label="play/pause"
          >
            <span className={styles.playCore}>
              {playing ? (
                <svg
                  viewBox="0 0 24 24"
                  width="30"
                  height="30"
                  fill="currentColor"
                  stroke="none"
                >
                  <rect x="7" y="5.5" width="3.4" height="13" rx="1.2" />
                  <rect x="13.6" y="5.5" width="3.4" height="13" rx="1.2" />
                </svg>
              ) : (
                <svg
                  viewBox="0 0 24 24"
                  width="30"
                  height="30"
                  fill="currentColor"
                  stroke="none"
                >
                  <path d="M8 5.5v13l11-6.5-11-6.5z" />
                </svg>
              )}
            </span>
          </button>
          <button className={styles.btn} onClick={onNext} aria-label="next">
            <svg
              viewBox="0 0 24 24"
              width="26"
              height="26"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="M6 6v12M16 12L8 6v12l8-6z" />
            </svg>
          </button>
        </div>

        <div className={styles.volume}>
          {volumeAvailable && (
            <>
              <span className={styles.volIcon} aria-hidden>
                <svg
                  viewBox="0 0 24 24"
                  width="20"
                  height="20"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.6"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <path d="M4 9v6h4l5 4V5L8 9H4zM16.5 8.5a5 5 0 0 1 0 7M19 6a8 8 0 0 1 0 12" />
                </svg>
              </span>
              <input
                className={styles.volRange}
                type="range"
                min={0}
                max={100}
                value={vol.local}
                style={{ ["--val" as string]: vol.local }}
                onPointerDown={vol.begin}
                onChange={(e) => vol.move(Number(e.target.value))}
                onPointerUp={vol.end}
                onPointerCancel={vol.end}
                aria-label="Volume"
              />
            </>
          )}
        </div>
      </div>
    </div>
  );
}
