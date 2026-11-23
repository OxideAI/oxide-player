import { folderKey } from "./util";
export interface AlbumTrack {
  id: number;
  uri: string;
  path: string;
}
export function libraryAlbumKeys(tracks: AlbumTrack[]): Map<number, string> {
  const roots = new Map<string, Set<string>>();
  for (const track of tracks) {
    const key = folderKey(track.uri);
    if (!roots.has(key)) roots.set(key, new Set());
    roots.get(key)!.add(folderKey(track.path));
  }
  return new Map(
    tracks.map((track) => {
      const relative = folderKey(track.uri);
      return [track.id, roots.get(relative)!.size > 1 ? folderKey(track.path) : relative];
    })
  );
}
export function resolveAlbumKey(
  reference: string,
  tracks: AlbumTrack[],
  trackId?: number
): string | null {
  const keys = libraryAlbumKeys(tracks);
  if (trackId !== undefined && keys.has(trackId)) return keys.get(trackId)!;
  const cueStem = reference.replace(/\.cue\/track\d+$/, "");
  const matches = tracks.filter(
    (track) =>
      track.uri === reference ||
      track.path === reference ||
      folderKey(track.uri) === reference ||
      folderKey(track.path) === reference ||
      (cueStem !== reference &&
        [track.uri, track.path].some((path) => path.replace(/\.[^./]+$/, "") === cueStem))
  );
  const matchedKeys = new Set(matches.map((track) => keys.get(track.id)!));
  if (matchedKeys.size > 1) return null;
  if (matchedKeys.size === 1) return [...matchedKeys][0];
  return reference;
}
