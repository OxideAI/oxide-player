import { describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { NowPlaying } from "./NowPlaying";
import { useKeyboardShortcuts } from "./useKeyboardShortcuts";
import type { PlayerStatus } from "../types";

describe("player keyboard controls", () => {
  it("seeks from the slider without invoking global next or previous", () => {
    const status: PlayerStatus = {
      state: "playing",
      elapsed: 10,
      duration: 80,
      volume: 50,
      outputs: [],
      error: null,
      current_song: null,
      random: false,
    };
    const next = vi.fn(),
      prev = vi.fn(),
      seek = vi.fn(),
      noop = vi.fn();
    function Player() {
      useKeyboardShortcuts({
        status,
        onNext: next,
        onPrev: prev,
        onSeek: seek,
        onVolume: noop,
        onTogglePlay: noop,
        onToggleKiosk: noop,
        onToggleShuffle: noop,
        onToggleMute: noop,
        onHelp: noop,
        onFeedback: noop,
        onSearch: noop,
      });
      return (
        <NowPlaying
          status={status}
          queue={null}
          onNext={next}
          onPrev={prev}
          onSeek={seek}
          onVolume={noop}
          onTogglePlay={noop}
          onOpenAlbum={noop}
        />
      );
    }
    render(<Player />);
    const slider = screen.getByRole("slider", { name: "Seek" });
    fireEvent.keyDown(slider, { key: "ArrowRight" });
    expect(seek).toHaveBeenCalledWith(15);
    expect(next).not.toHaveBeenCalled();
    fireEvent.keyDown(slider, { key: "ArrowLeft" });
    expect(seek).toHaveBeenCalledWith(5);
    expect(prev).not.toHaveBeenCalled();
    fireEvent.keyDown(document, { key: "ArrowRight" });
    fireEvent.keyDown(document, { key: "ArrowLeft" });
    expect(next).toHaveBeenCalledTimes(1);
    expect(prev).toHaveBeenCalledTimes(1);
  });
});
