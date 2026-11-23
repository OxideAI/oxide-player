import { it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { SearchView } from "./SearchView";
import type { Track } from "../types";
import { api } from "../api";
vi.mock("../api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api")>();
  return { ...actual, api: { ...actual.api, library: vi.fn() } };
});
it("opens the search result using its track identity rather than metadata album title", async () => {
  const track: Track = {
    id: 7,
    uri: "Miles/Folder/song.flac",
    path: "/usb/Miles/Folder/song.flac",
    title: "Song",
    artist: "Miles",
    album: "Kind of Blue",
    album_artist: null,
    genre: null,
    year: null,
    track: 1,
    duration: 180,
    format: "FLAC",
    sample_rate: null,
    bit_depth: null,
    channels: null,
    has_cover: false,
    cover_key: null,
    cue_index: null,
    start_time: null,
    end_time: null,
    file_mtime: null,
    source: "/usb",
    cue_start: null,
    art_url: null,
  };
  vi.mocked(api.library).mockResolvedValue([track]);
  const open = vi.fn();
  render(
    <SearchView
      query="Miles"
      nowPlayingId={null}
      isPlaying={false}
      onBack={vi.fn()}
      onOpenAlbum={open}
    />
  );
  fireEvent.click(await screen.findByText("Kind of Blue"));
  expect(open).toHaveBeenCalledWith(track.uri, track.id);
});
