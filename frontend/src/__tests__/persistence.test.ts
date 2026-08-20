import { describe, it, expect, beforeEach, vi } from "vitest";

function localStorageGetter(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

const STORAGE_PREFIX = "oxide:";

function getItem(key: string): string | null {
  try {
    return localStorage.getItem(STORAGE_PREFIX + key);
  } catch {
    return null;
  }
}

function setItem(key: string, value: string): void {
  try {
    localStorage.setItem(STORAGE_PREFIX + key, value);
  } catch {
    /* quota exceeded - ignore */
  }
}

function removeItem(key: string): void {
  try {
    localStorage.removeItem(STORAGE_PREFIX + key);
  } catch {
    /* ignore */
  }
}

function restoreTab(saved: string | null): string {
  const valid = ["library", "playlists", "settings"];
  return saved !== null && valid.includes(saved) ? saved : "library";
}

function isStaleFolderKey(key: string | null, folders: string[]): boolean {
  if (key === null || folders.length === 0) return false;
  return !folders.includes(key);
}

beforeEach(() => {
  localStorage.clear();
});

describe("localStorage persistence", () => {
  it("round-trips a value", () => {
    setItem("tab", "playlists");
    expect(getItem("tab")).toBe("playlists");
  });

  it("returns null for missing key", () => {
    expect(getItem("tab")).toBeNull();
  });

  it("removes a key", () => {
    setItem("album", "some/folder");
    removeItem("album");
    expect(getItem("album")).toBeNull();
  });

  it("returns null when localStorage throws", () => {
    const orig = Storage.prototype.getItem;
    Storage.prototype.getItem = vi.fn(() => {
      throw new Error("quota");
    });
    expect(localStorageGetter("oxide:tab")).toBeNull();
    Storage.prototype.getItem = orig;
  });
});

describe("tab restoration", () => {
  it("returns valid tab from storage", () => {
    expect(restoreTab("library")).toBe("library");
    expect(restoreTab("playlists")).toBe("playlists");
    expect(restoreTab("settings")).toBe("settings");
  });

  it("falls back to library for unknown tab", () => {
    expect(restoreTab("kiosk")).toBe("library");
    expect(restoreTab("")).toBe("library");
  });

  it("falls back to library for null", () => {
    expect(restoreTab(null)).toBe("library");
  });
});

describe("stale folder key detection", () => {
  const folders = ["album/a", "album/b", "album/c"];

  it("returns false when key exists in folders", () => {
    expect(isStaleFolderKey("album/a", folders)).toBe(false);
    expect(isStaleFolderKey("album/b", folders)).toBe(false);
  });

  it("returns true when key does not exist", () => {
    expect(isStaleFolderKey("album/z", folders)).toBe(true);
  });

  it("returns false when key is null", () => {
    expect(isStaleFolderKey(null, folders)).toBe(false);
  });

  it("returns false when folders list is empty", () => {
    expect(isStaleFolderKey("album/a", [])).toBe(false);
  });
});
