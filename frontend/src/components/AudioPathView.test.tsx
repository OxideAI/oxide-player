import { describe, expect, it } from "vitest";
import { reduceAudioPath, stableOutputIdentity } from "./AudioPathView";
import type { DeviceOutput, PlayerStatus } from "../types";

const output = (overrides: Partial<DeviceOutput> = {}): DeviceOutput => ({
  id: 1,
  name: "USB DAC",
  enabled: true,
  role: "playback",
  selectable: true,
  selection_key: "alsa:USB DAC",
  configured: true,
  available: true,
  connected: null,
  active: true,
  dsp_supported: true,
  dsp_enabled: false,
  ...overrides,
});

const status = (overrides: Partial<PlayerStatus> = {}): PlayerStatus => ({
  state: "playing",
  volume: 80,
  elapsed: 1,
  duration: 100,
  outputs: [{ id: 1, name: "USB DAC", enabled: true }],
  error: null,
  current_song: null,
  random: false,
  ...overrides,
});

describe("AudioPathView route reduction", () => {
  it.each([
    ["playing", "healthy"],
    ["paused", "paused"],
    ["stopped", "stopped"],
  ] as const)("maps %s player state to %s", (playerState, expected) => {
    expect(
      reduceAudioPath({
        status: status({ state: playerState }),
        outputs: [output()],
        connected: true,
      }).state,
    ).toBe(expected);
  });

  it("prioritizes stale status over an MPD error after disconnect", () => {
    expect(
      reduceAudioPath({
        status: status({ error: "decoder failed" }),
        outputs: [output()],
        connected: false,
        connectionError: "socket closed",
      }).state,
    ).toBe("stale");
  });

  it("surfaces MPD errors while the status connection is healthy", () => {
    expect(
      reduceAudioPath({
        status: status({ error: "No such song" }),
        outputs: [output()],
        connected: true,
      }).state,
    ).toBe("mpd-error");
  });

  it("distinguishes unavailable, zero-output, and multiple-output states", () => {
    expect(
      reduceAudioPath({ status: null, outputs: [], connected: false }).state,
    ).toBe("unavailable");
    expect(
      reduceAudioPath({ status: status(), outputs: [], connected: true }).state,
    ).toBe("zero-output");
    expect(
      reduceAudioPath({
        status: status({
          outputs: [
            { id: 1, name: "USB DAC", enabled: true },
            { id: 2, name: "Bedroom", enabled: true },
          ],
        }),
        outputs: [
          output(),
          output({ id: 2, name: "Bedroom", selection_key: "alsa:Bedroom" }),
        ],
        connected: true,
      }).state,
    ).toBe("multiple-output");
  });

  it("keeps route health visible from a live snapshot while details are unavailable", () => {
    const result = reduceAudioPath({
      status: status(),
      outputs: [],
      connected: true,
      detailsReady: false,
    });
    expect(result.state).toBe("healthy");
    expect(result.enabledOutputs[0].name).toBe("USB DAC");
  });

  it("uses stable selection identity rather than a numeric runtime id", () => {
    expect(stableOutputIdentity(output({ id: 4 }))).toBe("alsa:USB DAC");
    expect(stableOutputIdentity(output({ id: 99 }))).toBe(
      stableOutputIdentity(output({ id: 4 })),
    );
  });
});
