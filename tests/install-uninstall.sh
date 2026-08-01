#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

export OXIDE_INSTALLER_TEST=1
export BIN_DIR="$tmp_dir/usr/local/bin"
export SHARE_DIR="$tmp_dir/usr/local/share/oxide-player"
export CONFIG_DIR="$tmp_dir/etc/oxide-player"
export DATA_DIR="$tmp_dir/var/lib/oxide-player"
export MPD_MUSIC_DIR="$DATA_DIR/music"
export MPD_CONFIG="$tmp_dir/etc/mpd.conf"
export AIRPLAY_CONFIG="$CONFIG_DIR/shairport-sync.conf"
export ASOUND_CONFIG="$tmp_dir/etc/asound.conf"
export SAMBA_CONFIG="$tmp_dir/etc/samba/smb.conf"
export SYSTEMD_DIR="$tmp_dir/etc/systemd/system"
export SERVICE_USER="oxide"

mkdir -p "$BIN_DIR" "$SHARE_DIR/dist" "$CONFIG_DIR" "$DATA_DIR" "$MPD_MUSIC_DIR" \
  "$SYSTEMD_DIR" "$(dirname "$SAMBA_CONFIG")"
touch "$BIN_DIR/oxide-player" "$SYSTEMD_DIR/oxide-player.service" "$SYSTEMD_DIR/camilladsp.service"
printf 'preserved config\n' > "$CONFIG_DIR/config.json"
printf 'library database\n' > "$DATA_DIR/library.db"
printf 'old mpd config\n' > "$MPD_CONFIG.pre-oxide"
printf 'oxide mpd config\n' > "$MPD_CONFIG"
printf 'old samba config\n' > "$SAMBA_CONFIG.pre-oxide"
printf 'oxide samba config\n' > "$SAMBA_CONFIG"
printf '# oxide-player: managed\npcm.oxide_loopback {}\n' > "$ASOUND_CONFIG"
printf 'alsa = { output_device = "oxide_loopback"; };\n' > "$AIRPLAY_CONFIG"
# shellcheck disable=SC1091
source "$repo_root/install.sh"

# The test runs on the developer host without changing its services or requiring root.
need_root() { :; }
check_linux() { :; }
systemctl() { :; }

do_uninstall

test ! -e "$BIN_DIR/oxide-player"
test ! -e "$SHARE_DIR"
test ! -e "$SYSTEMD_DIR/oxide-player.service"
test ! -e "$SYSTEMD_DIR/camilladsp.service"
test ! -e "$ASOUND_CONFIG"
test ! -e "$AIRPLAY_CONFIG"
test -f "$CONFIG_DIR/config.json"
test -f "$DATA_DIR/library.db"
test -d "$MPD_MUSIC_DIR"
test "$(cat "$SAMBA_CONFIG")" = 'old samba config'

printf 'installer uninstall test passed\n'
