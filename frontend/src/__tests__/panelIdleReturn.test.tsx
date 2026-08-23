import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { usePanelIdleReturn } from "../components/usePanelIdleReturn";

// Panel-mode idle auto-return (plan 2026-08-23 U1 / KTD4): only sessions
// launched with ?panel=1 may navigate themselves back to /kiosk after
// continuous stopped playback; paused never triggers, non-panel clients are
// inert, and an already-kiosk page must not reload-loop.
//
// jsdom cannot perform real navigations ("Not implemented: navigation"), so
// window.location is replaced by a plain writable mock; assigning pathname on
// it is observable, which is exactly what the hook does.

const PANEL_KEY = "oxide.panel";

function installLocationMock(pathname: string, search = "") {
  // jsdom's Location is [LegacyUnforgeable] but deletable under vitest's
  // realm (verified empirically); a plain object lets `pathname =` assignments
  // stick where the real one would attempt an unimplemented navigation.
  // @ts-expect-error deliberate unforgeable-global shim
  delete window.location;
  Object.defineProperty(window, "location", {
    value: { pathname, search, href: `http://localhost:3000${pathname}${search}` },
    writable: true,
    configurable: true,
  });
}

function armPanel(idleSeconds: number) {
  sessionStorage.setItem(PANEL_KEY, JSON.stringify({ panel: true, idleSeconds }));
}

function renderIdleHook(state: string) {
  const harness = renderHook(
    (props: { state: string }) => usePanelIdleReturn(props.state),
    { initialProps: { state } },
  );
  return {
    setState: (next: string) =>
      act(() => {
        harness.rerender({ state: next });
      }),
    elapse: (ms: number) => act(() => vi.advanceTimersByTime(ms)),
    unmount: harness.unmount,
  };
}

describe("usePanelIdleReturn", () => {
  beforeEach(() => {
    sessionStorage.clear();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    installLocationMock("/");
  });

  it("navigates to /kiosk once stopped continuously past the threshold", () => {
    installLocationMock("/library");
    armPanel(2);
    const h = renderIdleHook("stopped");
    h.elapse(1000);
    expect(window.location.pathname).toBe("/library");
    h.elapse(1000);
    expect(window.location.pathname).toBe("/kiosk");
  });

  it("persists panel flag from launch URL query params", () => {
    installLocationMock("/kiosk", "?panel=1&idle=7");
    renderIdleHook("stopped");
    expect(JSON.parse(sessionStorage.getItem(PANEL_KEY)!)).toEqual({
      panel: true,
      idleSeconds: 7,
    });
  });

  it("defaults idleSeconds to 600 when ?idle is absent", () => {
    installLocationMock("/kiosk", "?panel=1");
    renderIdleHook("stopped");
    expect(JSON.parse(sessionStorage.getItem(PANEL_KEY)!)).toEqual({
      panel: true,
      idleSeconds: 600,
    });
  });

  it("never navigates while paused, even far past the threshold", () => {
    installLocationMock("/library");
    armPanel(2);
    const h = renderIdleHook("paused");
    h.elapse(10_000);
    expect(window.location.pathname).toBe("/library");
  });

  it("is inert without the panel flag regardless of elapsed time", () => {
    installLocationMock("/library");
    const h = renderIdleHook("stopped");
    h.elapse(60_000);
    expect(window.location.pathname).toBe("/library");
  });

  it("does not navigate when already on /kiosk (no reload loop)", () => {
    installLocationMock("/kiosk");
    armPanel(2);
    const h = renderIdleHook("stopped");
    h.elapse(10_000);
    expect(window.location.pathname).toBe("/kiosk");
  });

  it("resets the accumulator when playback leaves stopped, then re-arms", () => {
    installLocationMock("/library");
    armPanel(2);
    const h = renderIdleHook("stopped");

    // One second in, playback starts: progress must be thrown away.
    h.elapse(1000);
    h.setState("playing");
    h.elapse(5000);
    expect(window.location.pathname).toBe("/library");

    // Re-stop: needs the full threshold again from zero.
    h.setState("stopped");
    h.elapse(1000);
    expect(window.location.pathname).toBe("/library");
    h.elapse(1000);
    expect(window.location.pathname).toBe("/kiosk");
  });

  it("keeps arming from sessionStorage when the URL loses its params (Back button)", () => {
    installLocationMock("/kiosk", "?panel=1&idle=2");
    renderIdleHook("stopped").unmount();

    // Simulate Back-button navigation onto the full UI without query params.
    installLocationMock("/");
    armPanel(2); // flag survives via sessionStorage, not the URL
    const h = renderIdleHook("paused");
    h.setState("stopped");
    h.elapse(2000);
    expect(window.location.pathname).toBe("/kiosk");
  });
});
