import { useEffect, useRef } from "react";
import type { PlaybackState } from "../types";

// Panel-mode idle auto-return (plan 2026-08-23 U1 / KTD4). A session launched
// as the wall panel (`/kiosk?panel=1&idle=<seconds>`) may navigate itself back
// to /kiosk after continuous stopped playback past the idle threshold — so a
// touch-wake from the full UI always lands on the wall view. Phones and other
// clients never carry the flag and stay inert; a page already at /kiosk never
// reloads itself.

const PANEL_KEY = "oxide.panel";
const DEFAULT_IDLE_SECONDS = 600;

type PanelConfig = { panel: boolean; idleSeconds: number };

// Single clamp rule for both launch-URL and persisted values: finite numbers
// above zero win, everything else falls back to the default.
function normalizeIdle(raw: unknown): number {
  const n = Number(raw);
  return Number.isFinite(n) && n > 0 ? n : DEFAULT_IDLE_SECONDS;
}

function persistConfig(config: PanelConfig): void {
  try {
    sessionStorage.setItem(PANEL_KEY, JSON.stringify(config));
  } catch {
    // Storage being unavailable must not break arming for this page load.
  }
}

// Query params decide the session: `?panel=1` arms (and persists so Back-button
// navigation without the query stays armed), `?panel=0` explicitly disarms and
// clears any stored flag, anything else falls back to the persisted value.
function readPanelConfig(): PanelConfig | null {
  const params = new URLSearchParams(window.location.search);
  const panelParam = params.get("panel");
  if (panelParam === "1") {
    const config: PanelConfig = {
      panel: true,
      idleSeconds: normalizeIdle(params.get("idle")),
    };
    persistConfig(config);
    return config;
  }
  if (panelParam === "0") {
    try {
      sessionStorage.removeItem(PANEL_KEY);
    } catch {
      // Ignore storage failures; the session is simply not armed.
    }
    return null;
  }
  try {
    const raw = sessionStorage.getItem(PANEL_KEY);
    if (!raw) return null;
    const stored = JSON.parse(raw) as Partial<PanelConfig>;
    if (!stored?.panel) return null;
    return { panel: true, idleSeconds: normalizeIdle(stored.idleSeconds) };
  } catch {
    // Corrupted payload degrades to "not armed" rather than crashing arming.
    return null;
  }
}

export function usePanelIdleReturn(playbackState: PlaybackState): void {
  // The interval closure reads the latest state without re-subscribing.
  const stateRef = useRef(playbackState);
  stateRef.current = playbackState;

  useEffect(() => {
    const config = readPanelConfig();
    if (!config) return;
    if (window.location.pathname === "/kiosk") return;

    let stoppedSeconds = 0;

    // Human input counts as activity, mirroring the shell side where a touch
    // stamps the wake file and resets the blanker's clock. Without this, a
    // listener browsing the library while stopped would get hard-navigated
    // mid-tap exactly at the threshold.
    const markActivity = () => {
      stoppedSeconds = 0;
    };
    window.addEventListener("pointerdown", markActivity);
    window.addEventListener("keydown", markActivity);

    const interval = window.setInterval(() => {
      if (stateRef.current !== "stopped") {
        // Playing or paused holds the view indefinitely (KD3); paused in
        // particular never accumulates toward navigation.
        stoppedSeconds = 0;
        return;
      }
      stoppedSeconds += 1;
      if (stoppedSeconds >= config.idleSeconds) {
        stoppedSeconds = 0;
        if (window.location.pathname !== "/kiosk") {
          window.location.pathname = "/kiosk";
        }
      }
    }, 1000);
    return () => {
      window.clearInterval(interval);
      window.removeEventListener("pointerdown", markActivity);
      window.removeEventListener("keydown", markActivity);
    };
  }, []);
}
