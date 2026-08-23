export function fmtTime(s: number | null | undefined): string {
  if (s == null || !isFinite(s) || s < 0) s = 0;
  const m = Math.floor(s / 60);
  const sec = Math.floor(s % 60);
  return `${m}:${sec.toString().padStart(2, "0")}`;
}

export function displayTitle(t: { title: string | null; uri: string }): string {
  if (t.title) return t.title;
  const name = t.uri.split("/").pop() || t.uri;
  return name.replace(/\.[^./\\]+$/, "");
}

export function folderKey(uri: string): string {
  const idx = uri.lastIndexOf("/");
  return idx >= 0 ? uri.slice(0, idx) : "";
}

export function audioQuality(t: {
  format?: string | null;
  sample_rate?: number | null;
  bit_depth?: number | null;
  channels?: number | null;
}): string {
  const parts: string[] = [];
  if (t.sample_rate) {
    const k = t.sample_rate / 1000;
    parts.push(Number.isInteger(k) ? `${k} kHz` : `${k.toFixed(1)} kHz`);
  }
  if (t.bit_depth) parts.push(`${t.bit_depth}-bit`);
  if (t.channels) parts.push(`${t.channels}ch`);
  const q = parts.join(" / ");
  return t.format ? `${t.format.toUpperCase()} · ${q}` : q;
}

/** Best artwork for a now-playing row: library cover first, then radio
 *  station art, else nothing (callers show their placeholder). */
export function artUrlFor(song: {
  id: number;
  has_cover: boolean;
  cover_key: string | null;
  art_url: string | null;
} | null): string | null {
  if (!song) return null;
  if (song.has_cover) return `/api/cover/${song.cover_key ?? song.id}`;
  return song.art_url ?? null;
}
