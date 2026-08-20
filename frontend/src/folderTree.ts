import type { Track } from "./types";
import { displayTitle, folderKey } from "./util";

export interface FolderNode {
  absDir: string;
  basename: string;
  directCount: number;
  trackCount: number;
  coverKey: string | null;
  childCount: number;
}

export interface FolderTree {
  nodes: Map<string, FolderNode>;
  children: Map<string | null, string[]>;
  directTracks: Map<string, Track[]>;
  roots: string[];
}

export function absoluteDirOf(t: Track): string {
  const p = t.path?.trim() ?? "";
  if (p) {
    const idx = p.lastIndexOf("/");
    if (idx > 0) return p.slice(0, idx);
    if (idx === 0) return "/";
  }
  const source = t.source?.trim() ?? "";
  const relDir = folderKey(t.uri ?? "");
  if (source) {
    if (!relDir) return source.replace(/\/+$/, "") || source;
    return `${source.replace(/\/+$/, "")}/${relDir}`;
  }
  return relDir;
}

export function folderBasename(dir: string): string {
  if (!dir || dir === "/") return dir || "Library";
  const trimmed = dir.replace(/\/+$/, "");
  const idx = trimmed.lastIndexOf("/");
  return idx >= 0 ? trimmed.slice(idx + 1) : trimmed;
}

export function folderParent(dir: string): string | null {
  if (!dir || dir === "/") return null;
  const trimmed = dir.replace(/\/+$/, "");
  const idx = trimmed.lastIndexOf("/");
  if (idx <= 0) {
    if (idx === 0) return "/";
    return null;
  }
  return trimmed.slice(0, idx);
}

export function ancestors(dir: string): string[] {
  if (!dir) return [];
  const chain: string[] = [];
  let cur: string | null = dir;
  while (cur) {
    chain.push(cur);
    cur = folderParent(cur);
  }
  chain.reverse();
  return chain;
}

function trackOrder(a: Track, b: Track): number {
  const ai = a.cue_index ?? a.track ?? 0;
  const bi = b.cue_index ?? b.track ?? 0;
  if (ai !== bi) return ai - bi;
  return displayTitle(a).localeCompare(displayTitle(b));
}

export function buildFolderTree(tracks: Track[]): FolderTree {
  const directTracks = new Map<string, Track[]>();
  const nodes = new Map<string, FolderNode>();
  const children = new Map<string | null, string[]>();

  for (const t of tracks) {
    const dir = absoluteDirOf(t);
    const arr = directTracks.get(dir);
    if (arr) arr.push(t);
    else directTracks.set(dir, [t]);
  }
  for (const arr of directTracks.values()) arr.sort(trackOrder);

  const ensureNode = (absDir: string) => {
    if (nodes.has(absDir)) return;
    nodes.set(absDir, {
      absDir,
      basename: folderBasename(absDir),
      directCount: directTracks.get(absDir)?.length ?? 0,
      trackCount: 0,
      coverKey: null,
      childCount: 0,
    });
  };

  for (const dir of [...directTracks.keys()]) {
    const sample = directTracks.get(dir)?.[0];
    const source = sample?.source?.replace(/\/+$/, "") ?? null;
    let cur: string | null = dir;
    while (cur) {
      if (source && cur !== source && source.startsWith(cur + "/")) {
        break;
      }
      ensureNode(cur);
      if (source && cur === source) break;
      cur = folderParent(cur);
      if (cur !== null && source && cur !== source && source.startsWith(cur + "/")) {
        break;
      }
    }
  }

  for (const absDir of nodes.keys()) {
    const parent = folderParent(absDir);
    const key: string | null = parent !== null && nodes.has(parent) ? parent : null;
    const arr = children.get(key);
    if (arr) arr.push(absDir);
    else children.set(key, [absDir]);
  }
  for (const arr of children.values()) arr.sort((a, b) => a.localeCompare(b));

  const byDepth = [...nodes.keys()].sort((a, b) => b.split("/").length - a.split("/").length);
  for (const absDir of byDepth) {
    const node = nodes.get(absDir)!;
    const direct = directTracks.get(absDir) ?? [];
    let count = direct.length;
    let cover: string | null = direct.find((t) => t.has_cover && t.cover_key)?.cover_key ?? null;
    const kids = children.get(absDir) ?? [];
    for (const child of kids) {
      const childNode = nodes.get(child)!;
      count += childNode.trackCount;
      if (!cover && childNode.coverKey) cover = childNode.coverKey;
    }
    node.trackCount = count;
    node.coverKey = cover;
    node.childCount = kids.length;
  }

  const roots = [...(children.get(null) ?? [])];
  return { nodes, children, directTracks, roots };
}

export function sortedSubfolders(dir: string | null, tree: FolderTree): string[] {
  return [...(tree.children.get(dir) ?? [])];
}

export function descendantTracks(dir: string | null, tree: FolderTree): Track[] {
  const out: Track[] = [];
  const seen = new Set<number>();
  const pushDir = (d: string) => {
    const direct = tree.directTracks.get(d) ?? [];
    for (const t of direct) {
      if (!seen.has(t.id)) {
        seen.add(t.id);
        out.push(t);
      }
    }
  };
  if (dir === null) {
    const dfs = (parent: string | null) => {
      for (const child of sortedSubfolders(parent, tree)) {
        pushDir(child);
        dfs(child);
      }
    };
    dfs(null);
    return out;
  }
  const dfs = (cur: string) => {
    pushDir(cur);
    for (const child of sortedSubfolders(cur, tree)) dfs(child);
  };
  if (tree.nodes.has(dir)) dfs(dir);
  else pushDir(dir);
  return out;
}
