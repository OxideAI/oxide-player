import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import type { PlayerStatus, TrackRef } from "../types";

vi.mock("../api", () => ({
  api: {
    coverFor: vi.fn(() => null),
    coverUrl: vi.fn(() => ""),
    getVizParams: vi.fn(async () => ({})),
    setVizParams: vi.fn(async () => {}),
  },
}));

import { NowPlaying } from "../components/NowPlaying";

// KioskView opens the visualizer WebSocket and mounts the canvas visualizer;
vi.mock("../components/Visualizer", () => ({
  Visualizer: () => <div />,
  DEFAULT_VIZ_PARAMS: {},
  VIZ_PRESETS: [],
}));
vi.mock("../components/VisualizerControls", () => ({
  VisualizerControls: () => <div />,
}));
vi.mock("../useVisualizer", () => ({
  useVisualizer: () => ({ frame: null }),
}));

import { KioskView } from "../components/KioskView";

const radioSong: TrackRef = {
  id: 0,
  uri: "https://stream.example.org/jfk",
  title: "Live Track",
  artist: "JFK Ibiza",
  album: null,
  has_cover: false,
  cover_key: null,
  format: null,
  sample_rate: null,
  bit_depth: null,
  channels: null,
  duration: null,
  cue_start: null,
  art_url: "https://station.example.org/logo.png",
};

function radioStatus(playing: boolean): PlayerStatus {
  return {
    state: playing ? "playing" : "paused",
    volume: 60,
    elapsed: 0,
    duration: 0,
    outputs: [],
    error: null,
    current_song: radioSong,
    random: false,
  };
}

const noop = () => {};

describe("radio station artwork", () => {
  it("NowPlaying shows the station art for streams without a library cover", () => {
    render(
      <NowPlaying
        status={radioStatus(true)}
        queue={null}
        onTogglePlay={noop}
        onNext={noop}
        onPrev={noop}
        onSeek={noop}
        onVolume={noop}
        onOpenAlbum={noop}
      />,
    );
    const img = screen.getByAltText("") as HTMLImageElement;
    expect(img.getAttribute("src")).toBe("https://station.example.org/logo.png");
    // Playing streams keep the glow pulse over the art.
    expect(document.querySelector('[class*="coverGlow"]')).not.toBeNull();
  });

  it("KioskView shows the station art when a stream is playing", () => {
    const { container } = render(
      <KioskView
        status={radioStatus(true)}
        queue={null}
        onTogglePlay={noop}
        onNext={noop}
        onPrev={noop}
        onSeek={noop}
        onVolume={noop}
        onOpenAlbum={noop}
      />,
    );
    const artLayer = container.querySelector('[style*="logo.png"]') as HTMLElement | null;
    expect(artLayer).not.toBeNull();
    expect(artLayer!.getAttribute("style")).toContain(
      "https://station.example.org/logo.png",
    );
  });

  it("falls back to the placeholder when the station has no artwork", () => {
    const bare: TrackRef = { ...radioSong, art_url: null };
    render(
      <NowPlaying
        status={{ ...radioStatus(false), current_song: bare }}
        queue={null}
        onTogglePlay={noop}
        onNext={noop}
        onPrev={noop}
        onSeek={noop}
        onVolume={noop}
        onOpenAlbum={noop}
      />,
    );
    expect(screen.queryByAltText("")).toBeNull();
    // The EQ placeholder renders instead (three animated bars).
    const placeholder = document.querySelector(
      '[class*="coverPlaceholder"]',
    ) as HTMLElement | null;
    expect(placeholder).not.toBeNull();
    expect(placeholder!.querySelectorAll("i").length).toBe(3);
  });
});
