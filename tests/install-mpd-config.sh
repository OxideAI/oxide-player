#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

export OXIDE_INSTALLER_TEST=1
export CONFIG_DIR="$tmp_dir/etc/oxide-player"
export DATA_DIR="$tmp_dir/var/lib/oxide-player"
export MPD_CONFIG="$tmp_dir/etc/mpd.conf"
export SERVICE_USER="$(id -un)"

mkdir -p "$CONFIG_DIR" "$DATA_DIR"

# A package-created empty /etc/mpd.conf is a real upgrade state. The repair
# path must restore the core local-library settings, not only append the
# managed output include.
: > "$MPD_CONFIG"

# shellcheck disable=SC1091
source "$repo_root/install.sh"
ensure_mpd_include

grep -Fqx "music_directory     \"$MPD_MUSIC_DIR\"" "$MPD_CONFIG"
grep -Fq "include" "$MPD_CONFIG"
grep -Fq "$DATA_DIR/mpd-outputs.d/*.conf" "$MPD_CONFIG"

printf 'installer MPD config include test passed\n'

