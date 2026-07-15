#!/usr/bin/env bash
#
# Oxide UI smoke test — drives the built frontend through agent-browser and
# asserts backend state via the HTTP API.
#
# Requires:
#   - agent-browser CLI on PATH (npm i -g agent-browser && agent-browser install)
#   - the Oxide backend running and serving the frontend (default http://127.0.0.1:8000)
#   - a populated library (so there is something to play)
#
# Usage:
#   tests/ui-smoke.sh [BASE_URL]
#   BASE_URL=http://127.0.0.1:8000 tests/ui-smoke.sh
#
# Exit code is non-zero if any step fails.

set -u

BASE="${1:-${BASE_URL:-http://127.0.0.1:8000}}"
AB="agent-browser"
export BASE AB

PASS=0
FAIL=0

# --- helpers ---------------------------------------------------------------

# api_check <python-expr>: succeed if expr (over /api/status JSON `d`) is truthy.
api_check() {
  curl -s "$BASE/api/status" \
    | python3 -c "import sys,json; d=json.load(sys.stdin); sys.exit(0 if ($1) else 1)"
}

# wait_api <python-expr> [timeout_s]: poll until expr true.
wait_api() {
  local expr="$1"; local timeout="${2:-8}"; local i=0
  while [ "$i" -lt "$timeout" ]; do
    if api_check "$expr" 2>/dev/null; then return 0; fi
    sleep 1; i=$((i+1))
  done
  return 1
}

# snap_grep <pattern>: true if the interactive snapshot contains the pattern.
snap_grep() {
  $AB snapshot -i 2>/dev/null | grep -q "$1"
}

# Functions are invoked from `bash -c` wrappers below, so export them for the
# subshells that `step` spawns.
export -f api_check wait_api snap_grep

# step <name> <command...>: run a check; record pass/fail by exit status.
step() {
  local name="$1"; shift
  if "$@"; then
    echo "  PASS  $name"
    PASS=$((PASS+1))
  else
    echo "  FAIL  $name"
    FAIL=$((FAIL+1))
  fi
}

# --- preconditions ---------------------------------------------------------

command -v "$AB" >/dev/null 2>&1 || { echo "agent-browser not found on PATH"; exit 2; }
curl -s -o /dev/null "$BASE/" || { echo "backend not reachable at $BASE"; exit 2; }

echo "Oxide UI smoke test — $BASE"
echo "----------------------------------------"

# --- 1. Library loads ------------------------------------------------------

step "open app + library renders" \
  bash -c "$AB open '$BASE/' >/dev/null 2>&1; snap_grep 'Search albums'"

# --- 2. Search + play an album through the UI ------------------------------

step "search filters library" \
  bash -c "$AB find placeholder 'Search albums, artists…' fill 'Patience' >/dev/null 2>&1; sleep 1; snap_grep 'Patience George Michael'"

step "open album from search" \
  bash -c "timeout 20 $AB find role button click --name 'Patience George Michael' >/dev/null 2>&1; sleep 3; snap_grep 'Album actions'"

step "play album via Album actions menu" \
  bash -c "timeout 20 $AB find role button click --name 'Album actions' >/dev/null 2>&1; sleep 1; \
            timeout 20 $AB find role button click --name 'Clear and play' >/dev/null 2>&1; \
            wait_api \"(d['state'] == 'playing')\" 8"

# Regression guard for the track-row click path (POST /api/playback/play), which
# previously failed with "No such song" because it passed the raw DB uri to MPD
# instead of resolving it to a playable path.
step "play endpoint accepts library uri (track-row path)" \
  bash -c "python3 - '$BASE' <<'PY'
import sys, json, urllib.request
base = sys.argv[1]
tracks = json.load(urllib.request.urlopen(base + '/api/library?q=Patience'))
uri = tracks[0]['uri']
req = urllib.request.Request(
    base + '/api/playback/play',
    data = json.dumps({'uri': uri}).encode(),
    headers = {'Content-Type': 'application/json'},
    method = 'POST',
)
urllib.request.urlopen(req)
PY
wait_api \"(d['state'] == 'playing')\" 8"

# --- 3. Transport / shuffle ------------------------------------------------

step "pause toggles state" \
  bash -c "sleep 2; $AB find role button click --name play/pause >/dev/null 2>&1; wait_api \"(d['state'] in ('paused','stopped'))\" 6"

step "resume playback" \
  bash -c "sleep 2; $AB find role button click --name play/pause >/dev/null 2>&1; wait_api \"(d['state'] == 'playing')\" 6"

step "shuffle on reflects in status" \
  bash -c "curl -s -X POST '$BASE/api/playback/shuffle' -H 'Content-Type: application/json' -d '{\"on\":true}' >/dev/null; sleep 1; api_check \"(d['random'] == True)\""

# --- 4. Queue panel --------------------------------------------------------

step "queue panel opens" \
  bash -c "$AB find role button click --name 'view queue' >/dev/null 2>&1; sleep 1; snap_grep 'Close queue'"

step "queue panel closes" \
  bash -c "$AB find role button click --name 'Close queue' >/dev/null 2>&1; sleep 1; snap_grep 'Close queue' && exit 1 || exit 0"

# --- 5. Playlists tab ------------------------------------------------------

step "playlists tab renders" \
  bash -c "$AB find role button click --name Playlists >/dev/null 2>&1; sleep 1; snap_grep 'New playlist name'"

# --- 6. Settings tab (Devices/DSP embedded) --------------------------------

step "settings tab renders" \
  bash -c "$AB find role button click --name Settings >/dev/null 2>&1; sleep 1; snap_grep 'MPD host'"

# --- 7. Kiosk mode ---------------------------------------------------------

step "kiosk mode opens" \
  bash -c "timeout 20 $AB find role link click --name 'Open kiosk mode' >/dev/null 2>&1; sleep 1; $AB get url 2>/dev/null | grep -q '/kiosk'"

step "kiosk returns to library" \
  bash -c "timeout 20 $AB find role link click --name 'Back to library' >/dev/null 2>&1; sleep 1; $AB get url 2>/dev/null | grep -q '127.0.0.1:8000/\$'"

# --- 8. No console errors --------------------------------------------------

step "no page/console errors during session" \
  bash -c "test -z \"\$($AB errors 2>/dev/null | grep -i error)\""

# --- summary ---------------------------------------------------------------

echo "----------------------------------------"
echo "PASS: $PASS   FAIL: $FAIL"
[ "$FAIL" -eq 0 ]
