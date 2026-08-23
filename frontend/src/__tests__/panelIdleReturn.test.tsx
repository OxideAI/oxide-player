import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { usePanelIdleReturn } from "../components/usePanelIdleReturn";
import type { PlaybackState } from "../types";

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

function renderIdleHook(state: PlaybackState) {
  const harness = renderHook(
    (props: { state: PlaybackState }) => usePanelIdleReturn(props.state),
    { initialProps: { state } },
  );
  return {
    setState: (next: PlaybackState) =>
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

  it("launch params override a previously persisted config", () => {
    // Pre-existing session armed with a different threshold must be
    // overridden by the fresh launch URL (#158).
    armPanel(600);
    installLocationMock("/library", "?panel=1&idle=30");
    const h = renderIdleHook("stopped");
    expect(JSON.parse(sessionStorage.getItem(PANEL_KEY)!).idleSeconds).toBe(30);
    h.elapse(30_000);
    expect(window.location.pathname).toBe("/kiosk");
  });

  it("falls back to the default idle when ?idle is invalid, zero or negative", () => {
    for (const bad of ["bogus", "0", "-5"]) {
      sessionStorage.clear();
      installLocationMock("/library", `?panel=1&idle=${bad}`);
      const h = renderIdleHook("stopped");
      // Nothing navigates on the first tick (a 0/NaN threshold would fire here).
      h.elapse(1000);
      expect(window.location.pathname).toBe("/library");
      // And the persisted config carries the clamped default.
      expect(JSON.parse(sessionStorage.getItem(PANEL_KEY)!).idleSeconds).toBe(600);
      h.unmount();
    }
  });

  it("treats corrupted or invalid sessionStorage payloads as not armed", () => {
    const payloads = [
      "not-json{{",
      JSON.stringify({ panel: false }),
      JSON.stringify({}),
    ];
    for (const payload of payloads) {
      sessionStorage.clear();
      sessionStorage.setItem(PANEL_KEY, payload);
      installLocationMock("/library");
      const h = renderIdleHook("stopped");
      h.elapse(60_000);
      expect(window.location.pathname).toBe("/library");
      h.unmount();
    }
  });

  it("clamps an invalid persisted idleSeconds to the default", () => {
    sessionStorage.setItem(
      PANEL_KEY,
      JSON.stringify({ panel: true, idleSeconds: -1 }),
    );
    installLocationMock("/library");
    const h = renderIdleHook("stopped");
    h.elapse(600_000);
    expect(window.location.pathname).toBe("/kiosk");
  });

  it("still arms and navigates when sessionStorage.setItem throws", () => {
    const spy = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("quota exceeded");
    });
    installLocationMock("/library", "?panel=1&idle=2");
    const h = renderIdleHook("stopped");
    h.elapse(2000);
    expect(window.location.pathname).toBe("/kiosk");
    spy.mockRestore();
  });

  it("disarms and clears storage on explicit ?panel=0", () => {
    armPanel(5);
    installLocationMock("/library", "?panel=0");
    const h = renderIdleHook("stopped");
    expect(sessionStorage.getItem(PANEL_KEY)).toBeNull();
    h.elapse(60_000);
    expect(window.location.pathname).toBe("/library");
  });

  it("resets the idle clock on user interaction while stopped", () => {
    installLocationMock("/library", "?panel=1&idle=5");
    const h = renderIdleHook("stopped");
    // Four seconds in, the listener taps the panel.
    h.elapse(4000);
    act(() => {
      window.dispatchEvent(new Event("pointerdown"));
    });
    h.elapse(4000);
    // Eight seconds elapsed since mount but only four since the tap.
    expect(window.location.pathname).toBe("/library");
    h.elapse(1000);
    expect(window.location.pathname).toBe("/kiosk");
  });

  it("stops accumulating entirely after unmount", () => {
    installLocationMock("/library", "?panel=1&idle=3");
    const h = renderIdleHook("stopped");
    h.unmount();
    h.elapse(60_000);
    expect(window.location.pathname).toBe("/library");
  });
});
