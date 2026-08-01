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
cat > "$MPD_CONFIG" <<'EOF'
music_directory "/music"
audio_output {
    type "alsa"
    name "Default"
}
EOF

# shellcheck disable=SC1091
source "$repo_root/install.sh"
ensure_mpd_include

grep -Fqx "include \"$DATA_DIR/mpd-outputs.d/*.conf\"" "$MPD_CONFIG"

printf 'installer MPD config include test passed\n'
