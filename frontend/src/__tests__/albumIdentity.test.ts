import { describe, it, expect } from "vitest";
import { libraryAlbumKeys, resolveAlbumKey } from "../albumIdentity";

describe("library album identity", () => {
  const tracks = [
    { id: 1, uri: "Miles/Folder/song.flac", path: "/usb/Miles/Folder/song.flac" },
    { id: 2, uri: "Other/image.flac", path: "/nas/Other/image.flac" },
  ];
  it("resolves library, absolute playlist and CUE references to the same folder", () => {
    expect(resolveAlbumKey(tracks[0].uri, tracks, 1)).toBe("Miles/Folder");
    expect(resolveAlbumKey(tracks[0].path, tracks)).toBe("Miles/Folder");
    expect(resolveAlbumKey("/nas/Other/image.cue/track0002", tracks)).toBe("Other");
    expect(resolveAlbumKey("Miles/Folder", tracks)).toBe("Miles/Folder");
  });
  it("separates sources with matching relative folders and rejects ambiguity", () => {
    const both = [...tracks, { id: 3, uri: tracks[0].uri, path: "/nas/Miles/Folder/song.flac" }];
    expect(libraryAlbumKeys(both).get(1)).toBe("/usb/Miles/Folder");
    expect(libraryAlbumKeys(both).get(3)).toBe("/nas/Miles/Folder");
    expect(resolveAlbumKey(tracks[0].uri, both)).toBeNull();
    expect(resolveAlbumKey(tracks[0].uri, both, 3)).toBe("/nas/Miles/Folder");
  });
});
