# Residual MPD xruns after the rescan/update branch fix — investigation state

Date: 2026-11-01 · Server: oxide-player (192.168.3.250, Ubuntu aarch64, MPD
0.24.4, library ~10k tracks on a USB disk formatted **exfat 1.0** (verified via
`lsblk -f`), mounted through fuseblk)

## Shipped, verified fixes

1. **Inverted `run_scan` branch** (`backend/src/api/mod.rs`): "Refresh" used to
   force MPD `rescan` (full in-process re-read of the whole library →
   `update: added/updating` storms → decoder starvation). Now refresh = `update`
   (mtime-only), full scan = `rescan`. Regression tests
   (`library_refresh_issues_mpd_update_not_rescan`,
   `library_scan_issues_mpd_rescan`) with a fake MPD server; RED verified
   against the old branch order. Deployed on the server at 12:26
   (binary sha256 `3f96feb2…`).
2. **`/etc/asound.conf` dmix buffer** 4096 → 12288 (~85 ms → ~250 ms) and the
   matching `install.sh` changes (both write paths + `--update` repair).
3. **Route ownership**: an orphan CamillaDSP (spawned by the oxide-player
   service during the deploy stop/restart window) held `hw:4,0` while MPD's
   direct SMSL output competed for it (`Device or resource busy` every open).
   Killed the orphan; MPD owns the PCM (owner_pid = mpd), busy errors stopped
   (13:xx had zero).
4. DevicesView USB buffer prefill 200 ms → 500 ms (+test).

## NOT fixed: residual xruns on direct playback

After the route reconciliation (~13:00), with **no scanner work visible in
backend logs** (inferred; see hypothesis note), no CamillaDSP, playback
continuous, xruns occurred at a steady ~0.6/min rate in the direct-route
window, clustering around track changes and also mid-track (13:16:14, 13:18:04,
13:20:21):

| Window      | xruns                   |
| ----------- | ----------------------- |
| 13:00–13:09 | 6                       |
| 13:10–13:19 | 8                       |
| 13:20–13:26 | 1 (13:20:21, mid-track) |

Exact anchored totals through 13:26 (measured, `grep -c` on MPD log):
13:00–13:29 → **15 xruns in ~26 min** of continuous direct-route playback
(six in 13:0x, eight in 13:1x, one in 13:2x); no new xruns 13:21–13:26.
Cumulative `grep -c "Decoder is too slow"` per hour: 10:xx → 6, 11:xx → 18,
12:xx → 88, ~13:26 → 15, day total 162 (session start ≈ 41 at 10:5x).

## Hypotheses (NOT verified; do not treat as RCA)

- **Per-second status poller DB work**: `resolve_current_song` spawns a
  `spawn_blocking` SQLite lambda every 1 s. `EXPLAIN QUERY PLAN` shows the
  query is an indexed `SEARCH tracks USING sqlite_autoindex_tracks_1
(uri=? AND cue_index>?)` with a `USE TEMP B-TREE FOR ORDER BY`; the 1,836
  figure is the total rows in `tracks` (a separate count, not rows scanned per
  query), so neither the rows-per-query nor the sort cost is measured. The
  hot RT worker sampled is not on the blocking pool that runs this query, so
  the link to the 10–90 % `tokio-rt-worker` CPU is **unproven**.
  `pwrite64`/`sendfile` syscall totals sampled on the hot thread were
  microseconds — visualizer-fifo contention equally unproven.
- Other candidates: exFAT-over-FUSE latency spikes during sustained reads
  (filesystem type verified via `lsblk -f`; timing behavior unmeasured), MPD
  0.24 `auto_update`-triggered inotify sweeps. The backend's journal shows no
  scanner work in windows where xruns occurred (e.g. 13:16–13:24 has zero
  backend journal entries), but backend logging does not cover all activity,
  so "scanner-free" is inferred, not proven for every xrun instant.

## Suggested next steps

1. Benchmark `track_by_uri_and_elapsed` (it does a temp-B-tree sort per plan).
   If latency is meaningful, evaluate a covering index (e.g.
   `(uri, cue_index, start_time, end_time)`) or rate-limiting
   `resolve_current_song` (cache by uri, only re-resolve when `current_id`
   changes).
2. Add `perf` to the server (or run `strace -c` + `perf record`) against the
   hot `tokio-rt-worker` thread during a live xrun cluster.
3. Test with visualizer_fft disabled (`data/config.json` / server config) for
   an A/B comparison of the fifo path.
4. Consider migrating the library mount to ext4 (the disk is exFAT 1.0 over
   FUSE — see header — which lacks stable latency guarantees for sustained
   reads; in-kernel ext4 would remove that variability).
