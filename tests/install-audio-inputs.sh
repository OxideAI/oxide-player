#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

export OXIDE_INSTALLER_TEST=1
export CONFIG_DIR="$tmp_dir/etc/oxide-player"
export DATA_DIR="$tmp_dir/var/lib/oxide-player"
export MPD_CONFIG="$tmp_dir/etc/mpd.conf"
export AIRPLAY_CONFIG="$CONFIG_DIR/shairport-sync.conf"
export ASOUND_CONFIG="$tmp_dir/etc/asound.conf"
export MPD_MUSIC_DIR="$DATA_DIR/music"
export CAMILLADSP_CONFIG="$tmp_dir/etc/camilladsp/config.yml"
export SERVICE_USER="$(id -un)"
export SYSTEMD_DIR="$tmp_dir/systemd"

mkdir -p "$CONFIG_DIR" "$DATA_DIR" "$SYSTEMD_DIR" "$tmp_dir/bin"
cat > "$tmp_dir/bin/aplay" <<'EOF'
#!/usr/bin/env bash
printf 'card 0: DAC [USB DAC], device 0: Playback [USB Playback]\n'
printf 'card 1: Loopback [Loopback], device 0: Loopback PCM\n'
EOF
cat > "$tmp_dir/bin/bluealsa" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
cat > "$tmp_dir/bin/systemctl" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "$tmp_dir/bin/aplay" "$tmp_dir/bin/bluealsa" "$tmp_dir/bin/systemctl"
export PATH="$tmp_dir/bin:$PATH"

# shellcheck disable=SC1091
source "$repo_root/install.sh"
setup_asound
write_mpd_config
setup_airplay
setup_bluetooth

grep -Fq 'pcm.oxide_loopback' "$ASOUND_CONFIG"
grep -Fq 'pcm.!default' "$ASOUND_CONFIG"
grep -Fqx '    mdns_backend = "avahi";' "$AIRPLAY_CONFIG"
grep -Fqx '    output_device = "oxide_loopback";' "$AIRPLAY_CONFIG"
grep -Fqx 'Type=dbus' "$SYSTEMD_DIR/oxide-bluealsa.service"
grep -Fqx 'BusName=org.bluealsa' "$SYSTEMD_DIR/oxide-bluealsa.service"
grep -Fqx 'User=root' "$SYSTEMD_DIR/oxide-bluealsa.service"
grep -Fqx "ExecStart=$tmp_dir/bin/bluealsa -S -p a2dp-source -p a2dp-sink" "$SYSTEMD_DIR/oxide-bluealsa.service"

printf 'installer audio input configuration test passed\n'
