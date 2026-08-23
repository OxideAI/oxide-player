import { useEffect, useRef } from "react";

// Panel-mode idle auto-return (plan 2026-08-23 U1 / KTD4). A session launched
// as the wall panel (`/kiosk?panel=1&idle=<seconds>`) may navigate itself back
// to /kiosk after continuous stopped playback past the idle threshold — so a
// touch-wake from the full UI always lands on the wall view. Phones and other
// clients never carry the flag and stay inert; a page already at /kiosk never
// reloads itself.

const PANEL_KEY = "oxide.panel";
const DEFAULT_IDLE_SECONDS = 600;

type PanelConfig = { panel: boolean; idleSeconds: number };

// Query params win over sessionStorage so a changed ?idle= takes effect on a
// fresh panel launch; the persisted flag keeps the hook armed after Back-button
// navigation drops the query string.
function readPanelConfig(): PanelConfig | null {
  const params = new URLSearchParams(window.location.search);
  if (params.get("panel")) {
    const parsedIdle = Number(params.get("idle"));
    const config: PanelConfig = {
      panel: true,
      idleSeconds:
        Number.isFinite(parsedIdle) && parsedIdle > 0
          ? parsedIdle
          : DEFAULT_IDLE_SECONDS,
    };
    try {
      sessionStorage.setItem(PANEL_KEY, JSON.stringify(config));
    } catch {
      // Storage being unavailable must not break arming for this page load.
    }
    return config;
  }
  try {
    const raw = sessionStorage.getItem(PANEL_KEY);
    if (!raw) return null;
    const stored = JSON.parse(raw) as Partial<PanelConfig>;
    if (!stored?.panel) return null;
    const idleSeconds =
      typeof stored.idleSeconds === "number" && stored.idleSeconds > 0
        ? stored.idleSeconds
        : DEFAULT_IDLE_SECONDS;
    return { panel: true, idleSeconds };
  } catch {
    return null;
  }
}

export function usePanelIdleReturn(playbackState: string): void {
  // The interval closure reads the latest state without re-subscribing.
  const stateRef = useRef(playbackState);
  stateRef.current = playbackState;

  useEffect(() => {
    const config = readPanelConfig();
    if (!config || !config.panel) return;
    if (window.location.pathname === "/kiosk") return;

    let stoppedSeconds = 0;
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
    return () => window.clearInterval(interval);
  }, []);
}
