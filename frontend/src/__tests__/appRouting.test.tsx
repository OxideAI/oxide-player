import { describe, it, expect, beforeEach } from "vitest";
import { parsePath, buildPath } from "../App";

// Bug-rule (ui-navigation skill): any change to Route parsing/building must
// keep the back button (popstate) and deep links working — covered here.

describe("App routing", () => {
  beforeEach(() => {
    window.history.pushState({}, "", "/");
  });

  it("parses /radio into the radio tab", () => {
    window.history.pushState({}, "", "/radio");
    expect(parsePath()).toEqual({ tab: "radio", album: null, libraryView: "albums", folderPath: null });
  });

  it("keeps Settings as one top-level route", () => {
    window.history.pushState({}, "", "/settings");
    expect(parsePath()).toEqual({ tab: "settings", album: null, libraryView: "albums", folderPath: null });
  });

  it("keeps album deep links working", () => {
    window.history.pushState({}, "", "/library/Artist%20%2F%20Album");
    expect(parsePath()).toEqual({ tab: "library", album: "Artist / Album", libraryView: "albums", folderPath: null });
  });

  it("builds /radio path", () => {
    expect(buildPath({ tab: "radio", album: null, libraryView: "albums", folderPath: null })).toBe("/radio");
  });

  it("builds album path with encoding", () => {
    expect(buildPath({ tab: "library", album: "A/B", libraryView: "albums", folderPath: null })).toBe("/library/A%2FB");
  });

  it("falls back to library for unknown paths", () => {
    window.history.pushState({}, "", "/nope");
    expect(parsePath()).toEqual({ tab: "library", album: null, libraryView: "albums", folderPath: null });
  });

  it("parses /library/folders root", () => {
    window.history.pushState({}, "", "/library/folders");
    expect(parsePath()).toEqual({ tab: "library", album: null, libraryView: "folders", folderPath: null });
  });

  it("parses /library/folders/<encoded> round-trip", () => {
    const abs = "/mnt/music1/Artist/Album #1";
    const enc = encodeURIComponent(abs);
    window.history.pushState({}, "", `/library/folders/${enc}`);
    expect(parsePath()).toEqual({ tab: "library", album: null, libraryView: "folders", folderPath: abs });
    expect(buildPath({ tab: "library", album: null, libraryView: "folders", folderPath: abs })).toBe(`/library/folders/${enc}`);
  });

  it("parses space and % in folder path", () => {
    const abs = "/mnt/music2/My Folder 100%";
    window.history.pushState({}, "", `/library/folders/${encodeURIComponent(abs)}`);
    expect(parsePath().folderPath).toBe(abs);
  });
});
