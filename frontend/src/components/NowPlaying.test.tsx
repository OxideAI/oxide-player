import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PlayerStatus } from "../types";
import { NowPlaying } from "./NowPlaying";

function status(volume: number | null): PlayerStatus {
  return {
    state: "stopped",
    volume,
    current_song: null,
    elapsed: 0,
    duration: 0,
    outputs: [],
    error: null,
    random: false,
  };
}

describe("NowPlaying controls", () => {
  afterEach(() => cleanup());
  it("animates the cover placeholder only during playback", () => {
    const props = {
      queue: null,
      onTogglePlay: vi.fn(),
      onNext: vi.fn(),
      onPrev: vi.fn(),
      onSeek: vi.fn(),
      onVolume: vi.fn(),
      onOpenAlbum: vi.fn(),
    };
    const { container, rerender } = render(<NowPlaying {...props} status={status(69)} />);
    const equalizer = container.querySelector("[class*='eq']");
    expect(equalizer?.className).not.toContain("eqPlaying");

    rerender(<NowPlaying {...props} status={{ ...status(69), state: "playing" }} />);
    expect(equalizer?.className).toContain("eqPlaying");
  });

  it("shows the volume slider when a switched output reports volume support", () => {
    const props = {
      queue: null,
      onTogglePlay: vi.fn(),
      onNext: vi.fn(),
      onPrev: vi.fn(),
      onSeek: vi.fn(),
      onVolume: vi.fn(),
      onOpenAlbum: vi.fn(),
    };
    const { queryByLabelText, getByLabelText, rerender } = render(
      <NowPlaying {...props} status={status(null)} />
    );

    expect(queryByLabelText("Volume")).toBeNull();

    rerender(<NowPlaying {...props} status={status(69)} />);

    expect(getByLabelText("Volume")).toHaveProperty("value", "69");
  });

  it("keeps shuffle and queue controls from stretching in flex layouts", () => {
    const props = {
      queue: null,
      onTogglePlay: vi.fn(),
      onNext: vi.fn(),
      onPrev: vi.fn(),
      onSeek: vi.fn(),
      onVolume: vi.fn(),
      onOpenAlbum: vi.fn(),
    };
    const { getByLabelText } = render(<NowPlaying {...props} status={status(69)} />);

    for (const label of ["shuffle queue", "view queue"]) {
      const button = getByLabelText(label);
      const style = getComputedStyle(button);
      expect(style.flexShrink).toBe("0");
      expect(style.aspectRatio).toBe("1 / 1");
    }
  });
});
