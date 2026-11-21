import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { TrackMenu } from "./TrackMenu";
import type { Track } from "../types";

function makeTrack(overrides: Partial<Track> = {}): Track {
  return {
    id: 1,
    uri: "file:///music/Album/track.flac",
    title: "Test Track",
    artist: "Test Artist",
    album: "Test Album",
    has_cover: false,
    cover_key: null,
    format: "FLAC",
    sample_rate: 44100,
    bit_depth: 16,
    channels: 2,
    duration: 180,
    cue_start: null,
    art_url: null,
    path: "/music/Album/track.flac",
    genre: "Rock",
    year: 2020,
    track: 1,
    album_artist: null,
    cue_index: null,
    start_time: null,
    end_time: null,
    file_mtime: 1600000000,
    source: null,
    ...overrides,
  };
}

/** Mimics the album-view context: the menu lives inside a transformed ancestor
 *  (.rowMenu has transform: scale, .row has transform on hover). Before the
 *  portal fix, position:fixed on the modal resolved against that tiny span so
 *  it rendered clipped/mis-positioned. */
function AlbumLike({ children }: { children: React.ReactNode }) {
  return (
    <div
      data-testid="album-row"
      style={{ position: "relative", transform: "translateX(3px)", width: 320 }}
    >
      {children}
    </div>
  );
}

describe("TrackMenu FileInfo modal", () => {
  it("renders File info in a portal on document.body, visible despite transformed ancestor", () => {
    render(
      <AlbumLike>
        <TrackMenu tracks={[makeTrack()]} />
      </AlbumLike>
    );

    // Open the 3-dot menu.
    fireEvent.click(screen.getByLabelText("More actions"));
    fireEvent.click(screen.getByText("File info"));

    // The modal content renders and (thanks to the portal) is not trapped inside
    // the transformed album-row ancestor. jsdom can't verify layout/clipping, so
    // the visual centering guarantee is covered by the browser check in #49.
    const loc = screen.getByText("Location");
    expect(loc).toBeTruthy();
    expect(screen.getByText("Full path")).toBeTruthy();
    expect(screen.getByText("/music/Album/track.flac")).toBeTruthy();
    expect(document.body.contains(loc)).toBe(true);
  });

  it("closes File info on scrim click", () => {
    render(
      <AlbumLike>
        <TrackMenu tracks={[makeTrack()]} />
      </AlbumLike>
    );
    fireEvent.click(screen.getByLabelText("More actions"));
    fireEvent.click(screen.getByText("File info"));
    expect(screen.getByText("Location")).toBeTruthy();

    const scrim = document.body.querySelector('[class*="trackMenuModal"]')!;
    fireEvent.click(scrim);
    expect(screen.queryByText("Location")).toBeNull();
  });
});

describe("TrackMenu AlbumInfo modal", () => {
  it("offers album metadata from the album-header menu", () => {
    const tracks = [
      makeTrack({ album_artist: "Test Album Artist" }),
      makeTrack({
        id: 2,
        uri: "file:///music/Album/track-2.m4a",
        path: "/music/Album/track-2.m4a",
        title: "Second Track",
        genre: "Jazz",
        format: "ALAC",
        duration: 120,
        track: 2,
        album_artist: "Test Album Artist",
      }),
    ];

    render(<TrackMenu tracks={tracks} albumName="Test Album" />);

    fireEvent.click(screen.getByLabelText("More actions"));
    fireEvent.click(screen.getByText("Album info"));

    expect(screen.getByRole("dialog", { name: "Test Album" })).toBeTruthy();
    expect(screen.getByText("Full album path")).toBeTruthy();
    expect(screen.getByText("/music/Album")).toBeTruthy();
    expect(screen.getByText("2020")).toBeTruthy();
    expect(screen.getByText("Rock, Jazz")).toBeTruthy();
    expect(screen.getByText("Test Album Artist")).toBeTruthy();
    expect(screen.getByText("FLAC, ALAC")).toBeTruthy();
  });

  it("does not offer Album info from an ordinary track menu", () => {
    render(<TrackMenu tracks={[makeTrack()]} />);

    fireEvent.click(screen.getByLabelText("More actions"));

    expect(screen.queryByText("Album info")).toBeNull();
  });
});
