import { beforeEach, describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { App } from "../App";
const library = vi.hoisted(() => [
  { id: 7, uri: "Artist/Folder/song.flac", path: "/usb/Artist/Folder/song.flac" },
]);

vi.mock("../ws", () => ({
  usePlayerStatus: () => ({ status: null, queue: null, notice: null, connected: true }),
}));
vi.mock("../useLibraryTracks", () => ({
  useLibraryTracks: () => ({
    tracks: library,
    loading: false,
    refreshing: false,
    error: null,
    setError: vi.fn(),
  }),
}));
vi.mock("../components/KioskView", () => ({
  KioskView: ({ onOpenAlbum }: { onOpenAlbum: (key: string) => void }) => (
    <button onClick={() => onOpenAlbum("Artist/Album")}>Kiosk album</button>
  ),
}));
vi.mock("../components/LibraryView", () => ({
  LibraryView: ({ album }: { album: string | null }) => <div>Library album: {album}</div>,
}));
vi.mock("../components/NowPlaying", () => ({ NowPlaying: () => null }));
vi.mock("../components/InstallPrompt", () => ({ InstallPrompt: () => null }));
vi.mock("../components/SearchBar", () => ({
  SearchBar: ({ onSearch }: { onSearch: (query: string) => void }) => (
    <button onClick={() => onSearch("Song")}>Search fixture</button>
  ),
}));
vi.mock("../components/SearchView", () => ({
  SearchView: ({ onOpenAlbum }: { onOpenAlbum: (uri: string, id?: number) => void }) => (
    <button onClick={() => onOpenAlbum(library[0].uri, 7)}>Search result album</button>
  ),
}));

describe("navigation across the kiosk boundary", () => {
  beforeEach(() => window.history.replaceState({}, "", "/kiosk"));
  it("opens the album in the library and restores kiosk on history navigation", () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Kiosk album" }));
    expect(screen.getByText("Library album: Artist/Album")).toBeTruthy();
    expect(window.location.pathname).toBe("/library/Artist%2FAlbum");
    window.history.replaceState({}, "", "/kiosk");
    fireEvent.popState(window);
    expect(screen.getByRole("button", { name: "Kiosk album" })).toBeTruthy();
  });
  it("closes search and opens the selected result's library folder", () => {
    window.history.replaceState({}, "", "/library");
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Search fixture" }));
    fireEvent.click(screen.getByRole("button", { name: "Search result album" }));
    expect(screen.queryByRole("button", { name: "Search result album" })).toBeNull();
    expect(screen.getByText("Library album: Artist/Folder")).toBeTruthy();
  });
});
