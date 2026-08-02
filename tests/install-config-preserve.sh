#!/usr/bin/env bash
# Regression: re-running the installer (fresh-install path) must preserve
# user-edited config — "Music library sources" (library_dirs), listen, etc. —
# while still filling in installer-managed keys that older configs lack.
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

# Simulate a previous install whose config the user edited in the web UI:
# two library sources and a custom listen address, predating the
# visualizer_fft / visualizer_capture_* keys.
cat > "$CONFIG_DIR/config.json" <<'JSON'
{
  "mpd_host": "127.0.0.1",
  "mpd_port": 6600,
  "listen": "0.0.0.0:8080",
  "data_dir": "/var/lib/oxide-player",
  "mpd_config": "/etc/mpd.conf",
  "mpd_music_directory": "/var/lib/oxide-player/music",
  "bluetooth_reconnect_on_startup": true,
  "library_dirs": ["/mnt/music1", "/mnt/music2"],
  "static_dir": "/usr/local/share/oxide-player/dist",
  "camilladsp_config_path": "/etc/camilladsp/config.yml",
  "camilladsp_ws_url": "ws://127.0.0.1:1234",
  "default_dsp_profiles": []
}
JSON

source "$repo_root/install.sh"
write_oxide_config

python3 - "$CONFIG_DIR/config.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as f:
    config = json.load(f)

assert config["library_dirs"] == ["/mnt/music1", "/mnt/music2"], \
    f"user library sources must survive reinstall, got {config['library_dirs']}"
assert config["listen"] == "0.0.0.0:8080", \
    f"user listen setting must survive reinstall, got {config['listen']}"
assert config["visualizer_fft"] is True, "installer-managed keys missing from old config must be filled in"
assert config["visualizer_capture_device"] == "hw:Loopback,1"
assert config["visualizer_capture_rate"] == 44100
PY

printf 'installer config preservation test passed\n'
