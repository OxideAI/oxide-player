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
grep -Fq 'name          "camilladsp-loopback"' "$MPD_CONFIG"
grep -Fq 'mixer_type    "software"' "$MPD_CONFIG"
test "$(grep -Fc 'bind_to_address     "/run/mpd/socket"' "$MPD_CONFIG")" -eq 1

# Repair path: existing config without the unix socket bind gets it added
# before the port line; run it twice to prove idempotency.
cat > "$MPD_CONFIG" <<'EOF'
music_directory "/music"
port                "6600"
port                "6600"
include "/var/lib/oxide-player/mpd-outputs.d/*.conf"

audio_output {
    type "alsa"
    name "camilladsp-loopback"
}
EOF
ensure_mpd_include >/dev/null 2>&1 || true
ensure_mpd_include >/dev/null 2>&1 || true
test "$(grep -Fc 'bind_to_address     "/run/mpd/socket"' "$MPD_CONFIG")" -eq 1
# Bind goes before the first port line.
test "$(grep -n 'bind_to_address     "/run/mpd/socket"' "$MPD_CONFIG" | head -1 | cut -d: -f1)" -lt \
     "$(grep -n 'port                "6600"' "$MPD_CONFIG" | head -1 | cut -d: -f1)"
grep -Fq 'include ' "$MPD_CONFIG"
grep -Fq 'mpd-outputs.d/*.conf' "$MPD_CONFIG"
grep -Fq 'name "camilladsp-loopback"' "$MPD_CONFIG"

cat > "$MPD_CONFIG" <<'EOF'
music_directory "/music"
include "/var/lib/oxide-player/mpd-outputs.d/*.conf"

audio_output {
    type "alsa"
    name "camilladsp-loopback"
    device "oxide_loopback"
}
EOF
ensure_mpd_loopback_output
grep -Fq 'mixer_type    "software"' "$MPD_CONFIG"

cat > "$MPD_CONFIG" <<'EOF'
music_directory "/music"
audio_output {
    type "alsa"
    name "camilladsp-loopback"
    device "oxide_loopback"
    mixer_type "hardware"
}
EOF
ensure_mpd_loopback_output
grep -Fq 'mixer_type    "software"' "$MPD_CONFIG"
cat > "$MPD_CONFIG" <<'EOF'
music_directory "/music"
audio_output {
    type "alsa"
    mixer_type "hardware"
    name "camilladsp-loopback"
    device "hw:Loopback,0"
}
EOF
ensure_mpd_loopback_output
test "$(grep -Fc 'mixer_type' "$MPD_CONFIG")" -eq 1
grep -Fq 'mixer_type    "software"' "$MPD_CONFIG"
# Buffer tuning is injected on repair so playback survives decoder/network
# stalls ("Decoder is too slow; playing silence to avoid xrun").
grep -Fq 'buffer_time   "200000"' "$MPD_CONFIG"
grep -Fq 'period_time   "50000"' "$MPD_CONFIG"

# Existing settings are not duplicated or overwritten.
cat > "$MPD_CONFIG" <<'EOF'
music_directory "/music"
audio_output {
    type "alsa"
    name "camilladsp-loopback"
    device "oxide_loopback"
    mixer_type "software"
    buffer_time "300000"
    period_time "25000"
}
EOF
ensure_mpd_loopback_output || true
test "$(grep -Fc 'buffer_time' "$MPD_CONFIG")" -eq 1
test "$(grep -Fc 'period_time' "$MPD_CONFIG")" -eq 1
grep -Fq 'buffer_time "300000"' "$MPD_CONFIG"
grep -Fq 'period_time "25000"' "$MPD_CONFIG"

# A block with only one of the two settings gets the other injected.
cat > "$MPD_CONFIG" <<'EOF'
music_directory "/music"
audio_output {
    type "alsa"
    name "camilladsp-loopback"
    device "oxide_loopback"
    mixer_type "hardware"
    period_time "25000"
}
EOF
ensure_mpd_loopback_output || true
grep -Fq 'mixer_type    "software"' "$MPD_CONFIG"
test "$(grep -Fc 'period_time' "$MPD_CONFIG")" -eq 1
grep -Fq 'period_time "25000"' "$MPD_CONFIG"
grep -Fq 'buffer_time   "200000"' "$MPD_CONFIG"

cat > "$MPD_CONFIG" <<'EOF'
music_directory "/music"
audio_output {
    type "alsa"
    name "camilladsp-loopback"
    device "hw:USB"
    mixer_type "hardware"
}
EOF
ensure_mpd_loopback_output || true
! grep -Fq 'mixer_type    "software"' "$MPD_CONFIG"
grep -Fq 'mixer_type "hardware"' "$MPD_CONFIG"



cat > "$MPD_CONFIG" <<'EOF'
music_directory "/music"
audio_output {
    type "alsa"
    name "USB DAC"
    device "hw:USB"
    mixer_type "hardware"
}
EOF
ensure_mpd_loopback_output || true
! grep -Fq 'mixer_type    "software"' "$MPD_CONFIG"
grep -Fq 'mixer_type "hardware"' "$MPD_CONFIG"
# User-managed outputs keep their own buffering choices.
! grep -Fq 'buffer_time' "$MPD_CONFIG"
! grep -Fq 'period_time' "$MPD_CONFIG"

printf 'installer MPD loopback mixer test passed\n'


cat > "$MPD_CONFIG" <<'EOF'
music_directory "/music"
audio_output {
    type "alsa"
}
EOF
ensure_mpd_include
test -s "$MPD_CONFIG"
grep -Fq "$DATA_DIR/mpd-outputs.d/*.conf" "$MPD_CONFIG"

printf 'installer MPD config include test passed\n'

# setup_mpd_systemd: provisions a drop-in granting MPD write access to the
# Oxide data dir + log dir (distro mpd.service has ProtectSystem=yes).
# The test redirects the unit/source dir and the drop-in target via
# OXIDE_MPD_SYSTEMD_UNITDIR / OXIDE_MPD_SYSTEMD_CONFDIR (install-time only;
# real installs use /usr/lib/systemd/system and /etc/systemd/system).
cat > "$tmp_dir/vendor-unit" <<'EOF'
[Unit]
Description=Music Player Daemon
[Service]
Type=notify
ProtectSystem=yes
EOF
mkdir -p "$tmp_dir/vendor/systemd/system"
cp "$tmp_dir/vendor-unit" "$tmp_dir/vendor/systemd/system/mpd.service"

OXIDE_MPD_SYSTEMD_UNITDIR="$tmp_dir/vendor/systemd/system" \
OXIDE_MPD_SYSTEMD_CONFDIR="$tmp_dir/etc-systemd/mpd.service.d" \
  setup_mpd_systemd
grep -Fq "ReadWritePaths=$DATA_DIR /var/log/oxide-player" \
  "$tmp_dir/etc-systemd/mpd.service.d/oxide.conf"
grep -Fq 'oxide-player: managed mpd ReadWritePaths' \
  "$tmp_dir/etc-systemd/mpd.service.d/oxide.conf"

# Idempotency: second call leaves exactly one file
OXIDE_MPD_SYSTEMD_UNITDIR="$tmp_dir/vendor/systemd/system" \
OXIDE_MPD_SYSTEMD_CONFDIR="$tmp_dir/etc-systemd/mpd.service.d" \
  setup_mpd_systemd
test "$(ls "$tmp_dir/etc-systemd/mpd.service.d" | wc -l)" -eq 1

printf 'installer MPD systemd drop-in test passed\n'

