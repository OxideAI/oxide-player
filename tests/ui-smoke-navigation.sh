#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export OXIDE_SMOKE_TEST=1 BASE_URL=http://localhost:8123
source "$repo_root/tests/ui-smoke.sh"
AB=fixture_browser
fixture_browser() {
  if [ "$*" = "find role button click --name Back" ]; then return 0; fi
  if [ "$*" = "get url" ]; then printf '%s/\n' "$BASE"; return 0; fi
  return 1
}
sleep() { :; }
kiosk_returns_to_library
printf 'kiosk smoke return uses the rendered Back button and configured base URL\n'
