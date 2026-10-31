import { useCallback, useEffect, useMemo, useState } from "react";
import type { AlbumTag } from "../types";
import { api } from "../api";
import styles from "./AlbumTags.module.css";

const MAX_KEY_LENGTH = 64;

interface Props {
  /** Album name as stored in the library DB (never `null`). */
  album: string;
  notify?: (msg: string) => void;
  onError?: (msg: string) => void;
}

/**
 * Tag editor for one album. System-backed values (scanner-seeded `genre`) are
 * rendered pinned and cannot be removed; user values are removable inline.
 * The composer accepts any key (custom keys allowed; `genre` merges with the
 * system-backed row set) and multiple values per key.
 */
export function AlbumTags({ album, notify, onError }: Props) {
  const [tags, setTags] = useState<AlbumTag[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [composing, setComposing] = useState(false);
  const [keyDraft, setKeyDraft] = useState("");
  const [valueDraft, setValueDraft] = useState("");
  const [saving, setSaving] = useState(false);

  const load = useCallback(async () => {
    try {
      setTags(await api.albumTags(album));
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      onError?.(e instanceof Error ? e.message : String(e));
    }
  }, [album, onError]);

  useEffect(() => {
    load();
  }, [load]);

  const byKey = useMemo(() => {
    const map = new Map<string, AlbumTag[]>();
    for (const t of tags ?? []) {
      const list = map.get(t.key) ?? [];
      list.push(t);
      map.set(t.key, list);
    }
    return map;
  }, [tags]);

  const [suggestions, setSuggestions] = useState<string[]>([]);
  useEffect(() => {
    const key = keyDraft.trim();
    if (!composing || !key) {
      setSuggestions([]);
      return;
    }
    let cancelled = false;
    api
      .albumTagValues(key)
      .then((values) => {
        if (cancelled) return;
        setSuggestions(values.filter((v) => !valueDraft.includes(v)));
      })
      .catch(() => {
        if (!cancelled) setSuggestions([]);
      });
    return () => {
      cancelled = true;
    };
  }, [composing, keyDraft, valueDraft]);

  const remove = async (t: AlbumTag) => {
    try {
      await api.deleteAlbumTag(album, t.key, t.value);
      await load();
      notify?.(`Removed ${t.key}: ${t.value}`);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
      onError?.(msg);
    }
  };

  const commit = async () => {
    const key = keyDraft.trim();
    const values = valueDraft
      .split(",")
      .map((v) => v.trim())
      .filter(Boolean);
    if (!key || values.length === 0) {
      setComposing(false);
      return;
    }
    if (key.length > MAX_KEY_LENGTH) {
      setError(`Tag key must be at most ${MAX_KEY_LENGTH} characters`);
      return;
    }
    setSaving(true);
    try {
      const existing = (await api.albumTags(album)).filter((t) => t.key === key && !t.system);
      const merged = [...new Set([...existing.map((t) => t.value), ...values])];
      setTags(await api.setAlbumTags(album, key, merged));
      setComposing(false);
      setKeyDraft("");
      setValueDraft("");
      setError(null);
      notify?.(`Saved ${key}: ${values.join(", ")}`);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
      onError?.(msg);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className={styles.wrap} aria-busy={saving}>
      <div className={styles.chips}>
        {tags === null && !error && <span className={styles.hint}>…</span>}
        {error && <span className={styles.error}>{error}</span>}
        {tags !== null && byKey.size === 0 && !error && (
          <span className={styles.hint}>No tags yet</span>
        )}
        {[...byKey.entries()].flatMap(([, list]) =>
          list.map((t) => (
            <span
              key={`${t.key}:${t.value}`}
              className={t.system ? `${styles.chip} ${styles.systemChip}` : styles.chip}
              title={
                t.system
                  ? `${t.key}: ${t.value} (from file metadata; cannot be removed)`
                  : `${t.key}: ${t.value}`
              }
            >
              <span className={styles.chipKey}>{t.key}</span>
              <span className={styles.chipValue}>{t.value}</span>
              {!t.system && (
                <button
                  type="button"
                  className={styles.chipRemove}
                  aria-label={`Remove ${t.key} ${t.value}`}
                  onClick={() => remove(t)}
                >
                  ×
                </button>
              )}
            </span>
          ))
        )}
        {!composing && (
          <button type="button" className={styles.addChip} onClick={() => setComposing(true)}>
            + tag
          </button>
        )}
      </div>

      {composing && (
        <div className={styles.composer}>
          <input
            className={styles.input}
            placeholder="key (e.g. mood, artist, genre)"
            value={keyDraft}
            autoFocus
            maxLength={MAX_KEY_LENGTH}
            onChange={(e) => setKeyDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                commit();
              } else if (e.key === "Escape") {
                setComposing(false);
                setKeyDraft("");
                setValueDraft("");
              }
            }}
          />
          <input
            className={styles.input}
            placeholder="values, comma separated (e.g. Jazz, Funk)"
            value={valueDraft}
            onChange={(e) => setValueDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                commit();
              } else if (e.key === "Escape") {
                setComposing(false);
                setKeyDraft("");
                setValueDraft("");
              }
            }}
          />
          <button type="button" className={styles.saveBtn} disabled={saving} onClick={commit}>
            Save
          </button>
          <button
            type="button"
            className={styles.cancelBtn}
            onClick={() => {
              setComposing(false);
              setKeyDraft("");
              setValueDraft("");
            }}
          >
            Cancel
          </button>
        </div>
      )}
      {composing && suggestions.length > 0 && (
        <div className={styles.suggest}>
          {suggestions.slice(0, 8).map((s) => (
            <button
              key={s}
              type="button"
              className={styles.suggestBtn}
              onClick={() => {
                setKeyDraft(keyDraft.trim());
                setValueDraft(valueDraft.trim() ? `${valueDraft.trim()}, ${s}` : s);
              }}
            >
              {s}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
