import { useEffect, useRef } from "react";
import type { SpectrumFrame } from "../useVisualizer";
import styles from "./Visualizer.module.css";

interface Props {
  playing: boolean;
  frame: SpectrumFrame | null;
  params?: VizParams;
}

export type VizStyle = "bars" | "mirrored" | "circular" | "waveform" | "ring";

export const REFERENCE_VIZ_STYLES: ReadonlyArray<{
  id: VizStyle;
  label: string;
  icon: string;
  description: string;
}> = [
  {
    id: "bars",
    label: "Classic Bars",
    icon: "▥",
    description: "Frequency bars rising from the bottom",
  },
  {
    id: "mirrored",
    label: "Mirrored Bars",
    icon: "◈",
    description: "Symmetric bars expanding from the center",
  },
  {
    id: "circular",
    label: "Circular",
    icon: "◉",
    description: "Radial spectrum spikes around the album art",
  },
  {
    id: "waveform",
    label: "Waveform",
    icon: "〰",
    description: "Oscilloscope line shaped by the spectrum",
  },
  {
    id: "ring",
    label: "Ring of Fire",
    icon: "◎",
    description: "Glowing circular pulses around the stage",
  },
];

/// Live-tunable visualizer parameters. Exposed via the kiosk controls so the
/// look can be dialed in without recompiling.
export interface VizParams {
  style: VizStyle;
  bloomAlpha: number; // base halo opacity (0..1)
  bloomBeat: number; // halo opacity added by the beat pulse (0..1)
  bloomEnergy: number; // halo opacity added by audio level (0..1)
  bloomRadius: number; // halo radius as a multiple of the viewport diagonal
  barIdle: number; // minimum bar height fraction at rest (0..1)
  barPeak: number; // extra bar height fraction at full signal (0..1)
  barGap: number; // pixels between bars
  barRadius: number; // bar corner radius cap (px)
  phaseSpeed: number; // idle-pulse tempo (higher = faster)
  blur: number; // CSS blur in px
}

export const DEFAULT_VIZ_PARAMS: VizParams = {
  style: "bars",
  bloomAlpha: 0.28,
  bloomBeat: 0.16,
  bloomEnergy: 0.5,
  bloomRadius: 0.92,
  barIdle: 0.08,
  barPeak: 0.92,
  barGap: 3,
  barRadius: 6,
  phaseSpeed: 1.1,
  blur: 3,
};

export const VIZ_PRESETS: Record<VizStyle, VizParams> = {
  bars: DEFAULT_VIZ_PARAMS,
  mirrored: {
    ...DEFAULT_VIZ_PARAMS,
    style: "mirrored",
    bloomAlpha: 0.24,
    bloomBeat: 0.2,
    bloomEnergy: 0.55,
    barIdle: 0.06,
    barPeak: 0.86,
    barGap: 4,
    barRadius: 8,
  },
  circular: {
    ...DEFAULT_VIZ_PARAMS,
    style: "circular",
    bloomAlpha: 0.2,
    bloomBeat: 0.22,
    bloomEnergy: 0.62,
    barIdle: 0.08,
    barPeak: 0.95,
    barGap: 2,
    barRadius: 4,
    blur: 2,
  },
  waveform: {
    ...DEFAULT_VIZ_PARAMS,
    style: "waveform",
    bloomAlpha: 0.18,
    bloomBeat: 0.14,
    bloomEnergy: 0.72,
    barIdle: 0.04,
    barPeak: 0.9,
    phaseSpeed: 0.8,
    blur: 1,
  },
  ring: {
    ...DEFAULT_VIZ_PARAMS,
    style: "ring",
    bloomAlpha: 0.3,
    bloomBeat: 0.28,
    bloomEnergy: 0.75,
    barIdle: 0.1,
    barPeak: 0.9,
    barGap: 2,
    barRadius: 4,
    phaseSpeed: 1.35,
    blur: 4,
  },
};

const DEFAULT_BARS = 72;
const MAX_BARS = 256;

/** Resample backend FFT bins for any renderer width without dropping signal. */
export function getSpectrumTargets(
  _style: VizStyle,
  frame: SpectrumFrame | null,
  count: number,
): number[] {
  if (count <= 0) return [];
  const bins = frame?.bins?.filter((value) => Number.isFinite(value)) ?? [];
  if (bins.length === 0) {
    return new Array(count).fill(Math.max(0, Math.min(1, frame?.level ?? 0)));
  }
  if (bins.length === count)
    return bins.map((value) => Math.max(0, Math.min(1, value)));

  return Array.from({ length: count }, (_, index) => {
    const position =
      count === 1 ? 0 : (index * (bins.length - 1)) / (count - 1);
    const left = Math.floor(position);
    const right = Math.min(bins.length - 1, left + 1);
    const mix = position - left;
    return Math.max(0, Math.min(1, bins[left] * (1 - mix) + bins[right] * mix));
  });
}

export function hasSpectrumSignal(frame: SpectrumFrame | null): boolean {
  return (
    frame !== null &&
    (frame.level > 0 ||
      frame.bins.some((value) => Number.isFinite(value) && value > 0))
  );
}

export function Visualizer({ playing, frame, params }: Props) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const rafRef = useRef<number | null>(null);
  const reducedRef = useRef(false);
  const frameRef = useRef<SpectrumFrame | null>(frame);
  frameRef.current = frame;
  const paramsRef = useRef<VizParams>(params ?? DEFAULT_VIZ_PARAMS);
  const hasSignal = hasSpectrumSignal(frame);
  paramsRef.current = params ?? DEFAULT_VIZ_PARAMS;

  useEffect(() => {
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    reducedRef.current = mq.matches;
    const onMq = () => {
      reducedRef.current = mq.matches;
    };
    mq.addEventListener("change", onMq);
    return () => mq.removeEventListener("change", onMq);
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let accent = "#6ee7b7";
    let accent2 = "#8b9cff";
    const syncVars = () => {
      const computed = getComputedStyle(canvas);
      accent = computed.getPropertyValue("--accent").trim() || accent;
      accent2 = computed.getPropertyValue("--accent-2").trim() || accent2;
    };
    syncVars();

    const resize = () => {
      const dpr = window.devicePixelRatio || 1;
      const width = Math.max(1, Math.floor(canvas.clientWidth * dpr));
      const height = Math.max(1, Math.floor(canvas.clientHeight * dpr));
      if (canvas.width === width && canvas.height === height) return;
      canvas.width = width;
      canvas.height = height;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);

    const applyBlur = (px: number) => {
      canvas.style.filter = `blur(${px}px) saturate(1.5)`;
    };
    applyBlur(paramsRef.current.blur);

    const smoothed = new Float32Array(MAX_BARS);
    let phase = 0;

    const render = (animate: boolean) => {
      const p = paramsRef.current;
      if (canvas.style.filter !== `blur(${p.blur}px) saturate(1.5)`)
        applyBlur(p.blur);
      const width = canvas.clientWidth;
      const height = canvas.clientHeight;
      if (!animate) {
        ctx.clearRect(0, 0, width, height);
        return;
      }

      phase += (1 / 60) * p.phaseSpeed;
      const current = frameRef.current;
      const energy = Math.max(0, Math.min(1, current?.level ?? 0));
      const count = Math.min(current?.bins?.length || DEFAULT_BARS, MAX_BARS);
      const targets = getSpectrumTargets(p.style, current, count);
      for (let index = 0; index < targets.length; index++) {
        const target = targets[index];
        const previous = smoothed[index];
        const attack = target > previous ? 0.5 : 0.12;
        smoothed[index] += (target - previous) * attack;
      }

      ctx.clearRect(0, 0, width, height);
      const beat = 0.5 + 0.5 * Math.sin(phase * 1.6);
      const alpha = Math.min(
        1,
        p.bloomAlpha + beat * p.bloomBeat + energy * p.bloomEnergy,
      );
      const bloomRadius =
        Math.hypot(width, height) *
        (p.bloomRadius + beat * 0.08 + energy * 0.16);
      const bloom = ctx.createRadialGradient(
        width / 2,
        height * 0.52,
        0,
        width / 2,
        height * 0.52,
        bloomRadius,
      );
      bloom.addColorStop(0, hexToRgba(accent, alpha));
      bloom.addColorStop(0.45, hexToRgba(accent2, alpha * 0.72));
      bloom.addColorStop(1, "transparent");
      ctx.fillStyle = bloom;
      ctx.fillRect(0, 0, width, height);

      switch (p.style) {
        case "mirrored":
          drawMirrored(ctx, width, height, smoothed, count, p, accent, accent2);
          break;
        case "circular":
          drawCircular(
            ctx,
            width,
            height,
            smoothed,
            count,
            p,
            accent,
            accent2,
            phase,
          );
          break;
        case "waveform":
          drawWaveform(
            ctx,
            width,
            height,
            smoothed,
            count,
            p,
            accent,
            accent2,
            phase,
          );
          break;
        case "ring":
          drawRing(
            ctx,
            width,
            height,
            smoothed,
            count,
            p,
            accent,
            accent2,
            phase,
          );
          break;
        case "bars":
        default:
          drawBars(
            ctx,
            width,
            height,
            smoothed,
            count,
            p,
            accent,
            accent2,
            phase,
          );
          break;
      }
    };

    const loop = () => {
      render(true);
      rafRef.current = requestAnimationFrame(loop);
    };

    const active = playing || hasSignal;
    if (active && !reducedRef.current) {
      rafRef.current = requestAnimationFrame(loop);
    } else {
      render(active);
    }

    return () => {
      if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
      ro.disconnect();
    };
  }, [hasSignal, playing]);

  return <canvas ref={canvasRef} className={styles.canvas} aria-hidden />;
}

export function drawBars(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  values: Float32Array,
  count: number,
  p: VizParams,
  accent: string,
  accent2: string,
  phase: number,
) {
  if (count <= 0) return;
  const gap = p.barGap;
  const barWidth = Math.max(1, (width - gap * (count - 1)) / count);
  for (let index = 0; index < count; index++) {
    const idle = p.barIdle * (0.5 + 0.5 * Math.sin(phase * 1.3 + index * 0.5));
    const barHeight =
      height * (p.barIdle + Math.max(idle, values[index]) * p.barPeak);
    const x = index * (barWidth + gap);
    const y = height - barHeight;
    const gradient = ctx.createLinearGradient(0, y, 0, height);
    gradient.addColorStop(0, accent2);
    gradient.addColorStop(1, accent);
    ctx.fillStyle = gradient;
    roundRect(
      ctx,
      x,
      y,
      barWidth,
      barHeight,
      Math.min(barWidth / 2, p.barRadius),
    );
    ctx.fill();
  }
}

export function drawMirrored(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  values: Float32Array,
  count: number,
  p: VizParams,
  accent: string,
  accent2: string,
) {
  if (count <= 0) return;
  const gap = p.barGap;
  const barWidth = Math.max(1, (width - gap * (count - 1)) / count);
  const center = height / 2;
  for (let index = 0; index < count; index++) {
    const barHeight =
      height * (0.035 + (p.barIdle * 0.35 + values[index] * p.barPeak * 0.47));
    const x = index * (barWidth + gap);
    const gradient = ctx.createLinearGradient(
      0,
      center - barHeight,
      0,
      center + barHeight,
    );
    gradient.addColorStop(0, accent);
    gradient.addColorStop(0.5, accent2);
    gradient.addColorStop(1, accent);
    ctx.fillStyle = gradient;
    roundRect(
      ctx,
      x,
      center - barHeight,
      barWidth,
      barHeight,
      Math.min(barWidth / 2, p.barRadius),
    );
    ctx.fill();
    roundRect(
      ctx,
      x,
      center,
      barWidth,
      barHeight,
      Math.min(barWidth / 2, p.barRadius),
    );
    ctx.fill();
  }
}

function drawCircular(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  values: Float32Array,
  count: number,
  p: VizParams,
  accent: string,
  accent2: string,
  phase: number,
) {
  const cx = width / 2;
  const cy = height / 2;
  const inner = Math.min(width, height) * 0.23;
  const maxLength = Math.min(width, height) * 0.27;
  ctx.beginPath();
  ctx.arc(cx, cy, inner, 0, Math.PI * 2);
  ctx.strokeStyle = hexToRgba(accent, 0.4);
  ctx.lineWidth = 2;
  ctx.stroke();

  for (let index = 0; index < count; index++) {
    const angle = -Math.PI / 2 + (index / count) * Math.PI * 2;
    const pulse = p.barIdle * 0.3 + values[index] * p.barPeak;
    const start = inner;
    const end = inner + maxLength * pulse;
    const x1 = cx + Math.cos(angle) * start;
    const y1 = cy + Math.sin(angle) * start;
    const x2 = cx + Math.cos(angle) * end;
    const y2 = cy + Math.sin(angle) * end;
    ctx.beginPath();
    ctx.moveTo(x1, y1);
    ctx.lineTo(x2, y2);
    ctx.strokeStyle = index % 2 === 0 ? accent2 : accent;
    ctx.lineWidth = Math.max(2, p.barRadius * 0.65);
    ctx.lineCap = "round";
    ctx.stroke();
  }
  ctx.beginPath();
  ctx.arc(cx, cy, inner * (0.94 + Math.sin(phase) * 0.025), 0, Math.PI * 2);
  ctx.strokeStyle = accent;
  ctx.lineWidth = 1.5;
  ctx.stroke();
}

function drawWaveform(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  values: Float32Array,
  count: number,
  p: VizParams,
  accent: string,
  accent2: string,
  phase: number,
) {
  const center = height / 2;
  const amplitude = height * (0.14 + p.barPeak * 0.28);
  const gradient = ctx.createLinearGradient(0, 0, width, 0);
  gradient.addColorStop(0, accent);
  gradient.addColorStop(0.5, accent2);
  gradient.addColorStop(1, accent);
  ctx.strokeStyle = gradient;
  ctx.lineWidth = Math.max(2, p.barRadius * 0.55);
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.beginPath();
  for (let index = 0; index < count; index++) {
    const x = count === 1 ? 0 : (index / (count - 1)) * width;
    const phaseOffset = phase * 1.4 + index * 0.72;
    const y =
      center +
      Math.sin(phaseOffset) * amplitude * (p.barIdle * 0.3 + values[index]);
    if (index === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  }
  ctx.stroke();
  ctx.beginPath();
  ctx.moveTo(0, center);
  ctx.lineTo(width, center);
  ctx.strokeStyle = hexToRgba(accent, 0.16);
  ctx.lineWidth = 1;
  ctx.stroke();
}

function drawRing(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  values: Float32Array,
  count: number,
  p: VizParams,
  accent: string,
  accent2: string,
  phase: number,
) {
  const cx = width / 2;
  const cy = height / 2;
  const base = Math.min(width, height) * 0.28;
  ctx.beginPath();
  ctx.arc(cx, cy, base, 0, Math.PI * 2);
  ctx.strokeStyle = hexToRgba(accent2, 0.45);
  ctx.lineWidth = 2;
  ctx.stroke();

  for (let index = 0; index < count; index++) {
    const angle = -Math.PI / 2 + (index / count) * Math.PI * 2;
    const pulse = p.barIdle * 0.5 + values[index] * p.barPeak;
    const inner = base + 3;
    const outer = inner + Math.min(width, height) * 0.2 * pulse;
    const x1 = cx + Math.cos(angle) * inner;
    const y1 = cy + Math.sin(angle) * inner;
    const x2 = cx + Math.cos(angle) * outer;
    const y2 = cy + Math.sin(angle) * outer;
    ctx.beginPath();
    ctx.moveTo(x1, y1);
    ctx.lineTo(x2, y2);
    ctx.strokeStyle = index % 3 === 0 ? accent2 : accent;
    ctx.lineWidth = Math.max(2, p.barRadius * 0.7);
    ctx.lineCap = "round";
    ctx.stroke();
  }

  ctx.beginPath();
  ctx.arc(cx, cy, base * (1 + 0.05 * Math.sin(phase * 1.4)), 0, Math.PI * 2);
  ctx.strokeStyle = accent;
  ctx.lineWidth = 1;
  ctx.stroke();
}

function hexToRgba(hex: string, alpha: number): string {
  const m = hex.replace("#", "");
  if (m.length < 6) return `rgba(110,231,183,${alpha})`;
  const r = parseInt(m.slice(0, 2), 16);
  const g = parseInt(m.slice(2, 4), 16);
  const b = parseInt(m.slice(4, 6), 16);
  return `rgba(${r},${g},${b},${alpha})`;
}

function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
) {
  const rr = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + rr, y);
  ctx.arcTo(x + w, y, x + w, y + h, rr);
  ctx.arcTo(x + w, y + h, x, y + h, rr);
  ctx.arcTo(x, y + h, x, y, rr);
  ctx.arcTo(x, y, x + w, y, rr);
  ctx.closePath();
}
