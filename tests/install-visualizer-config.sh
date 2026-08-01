#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

export OXIDE_INSTALLER_TEST=1
export CONFIG_DIR="$tmp_dir/etc/oxide-player"
export DATA_DIR="$tmp_dir/var/lib/oxide-player"
export MPD_MUSIC_DIR="$DATA_DIR/music"
export LISTEN="0.0.0.0:80"
export CAMILLADSP_CONFIG="$tmp_dir/etc/camilladsp/config.yml"
export CAMILLADSP_WS="ws://127.0.0.1:1234"
export SERVICE_USER="$(id -un)"

mkdir -p "$CONFIG_DIR"
chown() { :; }
source "$repo_root/install.sh"
write_oxide_config

python3 - "$CONFIG_DIR/config.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as f:
    config = json.load(f)

assert config["bluetooth_reconnect_on_startup"] is True, "installed config must enable Bluetooth startup reconnect"
assert config["visualizer_fft"] is True, "installed config must enable FFT visualizer"
assert config["visualizer_capture_device"] == "hw:Loopback,1"
assert config["visualizer_capture_rate"] == 44100
PY

printf 'installer visualizer config test passed\n'
