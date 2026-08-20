import { useEffect, useRef, useState } from "react";

export interface SpectrumFrame {
  bins: number[];
  level: number;
}

/**
 * Live connection to `/api/visualizer`. Receives FFT spectrum frames
 * (`{ bins: number[], level: number }`) as JSON messages at ~40 fps. The
 * visualizer hooks into this instead of synthesizing motion so the bars reflect
 * the real audio signal — independent of player volume (the FFT magnitudes come
 * from the raw PCM, not the UI's volume value).
 *
 * Connects on mount, reconnects with backoff, and falls back to a flat zero
 * frame if the socket can't be opened (e.g. FFT disabled on the server).
 */
export function useVisualizer(enabled: boolean): SpectrumFrame | null {
  const [frame, setFrame] = useState<SpectrumFrame | null>(null);
  const stopped = useRef(false);

  useEffect(() => {
    if (!enabled) {
      setFrame(null);
      return;
    }
    stopped.current = false;
    let ws: WebSocket | null = null;
    let backoff = 1000;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const url = () => {
      const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
      return `${proto}//${window.location.host}/api/visualizer`;
    };

    const connect = () => {
      if (stopped.current) return;
      try {
        ws = new WebSocket(url());
      } catch {
        schedule();
        return;
      }
      ws.onmessage = (ev) => {
        try {
          const f = JSON.parse(ev.data) as SpectrumFrame;
          if (Array.isArray(f.bins)) setFrame(f);
        } catch {
          /* ignore malformed frame */
        }
      };
      ws.onopen = () => {
        backoff = 1000;
      };
      ws.onclose = () => schedule();
      ws.onerror = () => {
        if (ws) ws.close();
      };
    };

    const schedule = () => {
      if (stopped.current) return;
      const delay = backoff;
      backoff = Math.min(backoff * 2, 10000);
      timer = setTimeout(connect, delay);
    };

    connect();
    return () => {
      stopped.current = true;
      if (timer) clearTimeout(timer);
      if (ws) {
        ws.onclose = null;
        ws.close();
      }
    };
  }, [enabled]);

  return frame;
}
