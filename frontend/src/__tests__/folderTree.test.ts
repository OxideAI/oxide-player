import { describe, it, expect } from "vitest";
import { absoluteDirOf, ancestors, buildFolderTree, descendantTracks, folderBasename, folderParent, sortedSubfolders } from "../folderTree";
import type { Track } from "../types";

function track(over: Partial<Track> & { id: number; uri: string; path: string }): Track {
  return {
    id: over.id,
    uri: over.uri,
    path: over.path,
    title: (over as Track).title ?? null,
    artist: (over as Track).artist ?? null,
    album: (over as Track).album ?? null,
    has_cover: over.has_cover ?? false,
    cover_key: (over as Track).cover_key ?? null,
    format: null,
    sample_rate: null,
    bit_depth: null,
    channels: null,
    duration: null,
    cue_start: null,
    art_url: null,
    genre: null,
    year: null,
    track: (over as Track).track ?? null,
    album_artist: null,
    cue_index: (over as Track).cue_index ?? null,
    start_time: null,
    end_time: null,
    file_mtime: null,
    source: over.source ?? null,
  };
}

describe("folder helpers", () => {
  it("folderParent edge cases", () => {
    expect(folderBasename("/a/b/c")).toBe("c");
    expect(folderBasename("/")).toBe("/");
    expect(folderParent("/a/b/c")).toBe("/a/b");
    expect(folderParent("/a")).toBe("/");
    expect(folderParent("/")).toBe(null);
    expect(ancestors("/a/b/c")).toEqual(["/", "/a", "/a/b", "/a/b/c"]);
  });
  it("absoluteDirOf prefers path", () => {
    const t = track({ id: 1, uri: "Artist/Album/01.flac", path: "/mnt/music1/Artist/Album/01.flac", source: "/mnt/music1" });
    expect(absoluteDirOf(t)).toBe("/mnt/music1/Artist/Album");
  });
  it("absoluteDirOf fallback to source + rel", () => {
    const t = track({ id: 1, uri: "Artist/Album/01.flac", path: "", source: "/mnt/music1" });
    expect(absoluteDirOf(t)).toBe("/mnt/music1/Artist/Album");
  });
  it("buildFolderTree distinct mount roots", () => {
    const tracks = [
      track({ id: 1, uri: "Rock/Album/song.flac", path: "/mnt/music1/Rock/Album/song.flac", source: "/mnt/music1" }),
      track({ id: 2, uri: "Rock/Album/song.flac", path: "/mnt/music2/Rock/Album/song.flac", source: "/mnt/music2" }),
    ];
    const tree = buildFolderTree(tracks);
    expect(tree.nodes.has("/mnt/music1/Rock/Album")).toBe(true);
    expect(tree.nodes.has("/mnt/music2/Rock/Album")).toBe(true);
    expect(tree.roots.length).toBe(2);
  });
  it("descendantTracks recurses and sorts", () => {
    const tracks = [
      track({ id: 1, uri: "Artist/Album/Disc 1/02.flac", path: "/mnt/music1/Artist/Album/Disc 1/02.flac", source: "/mnt/music1", track: 2 }),
      track({ id: 2, uri: "Artist/Album/Disc 1/01.flac", path: "/mnt/music1/Artist/Album/Disc 1/01.flac", source: "/mnt/music1", track: 1 }),
      track({ id: 3, uri: "Artist/Album/Disc 2/01.flac", path: "/mnt/music1/Artist/Album/Disc 2/01.flac", source: "/mnt/music1", track: 1 }),
    ];
    const tree = buildFolderTree(tracks);
    const des = descendantTracks("/mnt/music1/Artist/Album", tree);
    expect(des.map((t) => t.id)).toEqual([2, 1, 3]);
  });
  it("sortedSubfolders lexicographic", () => {
    const tracks = [
      track({ id: 1, uri: "A/z/s.flac", path: "/src/A/z/s.flac", source: "/src" }),
      track({ id: 2, uri: "A/a/s.flac", path: "/src/A/a/s.flac", source: "/src" }),
    ];
    const tree = buildFolderTree(tracks);
    expect(sortedSubfolders("/src/A", tree)).toEqual(["/src/A/a", "/src/A/z"]);
  });
  it("empty library", () => {
    const tree = buildFolderTree([]);
    expect(tree.nodes.size).toBe(0);
    expect(tree.roots.length).toBe(0);
    expect(descendantTracks(null, tree)).toEqual([]);
  });
});
