import { useEffect, useRef, useState } from "react";
import type {
  PlaybackNotice,
  PlayerStatus,
  QueueResponse,
  StatusEvent,
} from "./types";
import { api } from "./api";

export interface PlayerState {
  status: PlayerStatus | null;
  queue: QueueResponse | null;
  notice: PlaybackNotice | null;
  connected: boolean;
  error: string | null;
  lastSnapshotAt: number | null;
}

const MAX_BACKOFF = 10000;

function wsUrl(): string {
  const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${window.location.host}/api/ws`;
}

/**
 * Single live connection to `/api/ws`. Seeds state from the first snapshot the
 * server sends on connect, then applies every pushed `StatusEvent`. Reconnects
 * with exponential backoff and re-seeds on reopen. Replaces the old
 * `setInterval` polls of `/api/status` + `/api/queue` (issue #3).
 *
 * If the socket can't be opened (e.g. an older backend without `/api/ws`, or a
 * transient network failure), it falls back to one-shot REST fetches of
 * `/api/status` + `/api/queue` so the UI still renders rather than sitting on
 * "Connecting…" forever.
 */
export function usePlayerStatus(): PlayerState {
  const [state, setState] = useState<PlayerState>({
    status: null,
    queue: null,
    notice: null,
    connected: false,
    error: null,
    lastSnapshotAt: null,
  });
  const backoff = useRef(1000);
  const stopped = useRef(false);

  useEffect(() => {
    stopped.current = false;
    let ws: WebSocket | null = null;
    let timer: ReturnType<typeof setTimeout> | null = null;
    let fallbackTimer: ReturnType<typeof setTimeout> | null = null;

    // One-shot REST fallback used only when the WS never connects.
    const restFallback = () => {
      if (stopped.current) return;
      Promise.all([
        api.status().catch(() => null),
        api.queue().catch(() => null),
      ])
        .then(([status, queue]) => {
          if (stopped.current) return;
          setState((s) => ({
            ...s,
            status: status ?? s.status,
            queue: queue ?? s.queue,
            lastSnapshotAt: status ? Date.now() : s.lastSnapshotAt,
          }));
        })
        .catch(() => {});
    };

    const connect = () => {
      if (stopped.current) return;
      try {
        ws = new WebSocket(wsUrl());
      } catch {
        restFallback();
        scheduleReconnect();
        return;
      }

      // If the socket hasn't opened shortly, try the REST fallback once so the
      // UI isn't blank while we keep retrying the socket.
      fallbackTimer = setTimeout(restFallback, 1500);

      ws.onopen = () => {
        if (fallbackTimer) clearTimeout(fallbackTimer);
        backoff.current = 1000;
        setState((s) => ({ ...s, connected: true, error: null }));
      };

      ws.onmessage = (ev) => {
        let event: StatusEvent;
        try {
          event = JSON.parse(ev.data);
        } catch {
          return;
        }
        if (event.type === "status") {
          setState((s) => ({
            ...s,
            status: event,
            lastSnapshotAt: Date.now(),
          }));
        } else if (event.type === "queue") {
          setState((s) => ({ ...s, queue: event }));
        } else if (event.type === "notice") {
          setState((s) =>
            s.notice?.id === event.id ? s : { ...s, notice: event },
          );
        }
      };

      ws.onerror = () => {
        if (fallbackTimer) clearTimeout(fallbackTimer);
        setState((s) => ({
          ...s,
          connected: false,
          error: "Connection to player lost — retrying…",
        }));
      };

      ws.onclose = () => {
        if (fallbackTimer) clearTimeout(fallbackTimer);
        setState((s) => ({ ...s, connected: false }));
        scheduleReconnect();
      };
    };

    const scheduleReconnect = () => {
      if (stopped.current) return;
      const delay = backoff.current;
      backoff.current = Math.min(backoff.current * 2, MAX_BACKOFF);
      timer = setTimeout(connect, delay);
    };

    connect();

    return () => {
      stopped.current = true;
      if (timer) clearTimeout(timer);
      if (fallbackTimer) clearTimeout(fallbackTimer);
      if (ws) {
        ws.onclose = null;
        ws.close();
      }
    };
  }, []);

  return state;
}
