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

cat > "$MPD_CONFIG" <<'EOF'
music_directory "/music"
include "/var/lib/oxide-player/mpd-outputs.d/*.conf"

audio_output {
    type "alsa"
    name "camilladsp-loopback"
    device "oxide_loopback"
}
EOF
ensure_mpd_loopback_mixer
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
ensure_mpd_loopback_mixer
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
ensure_mpd_loopback_mixer
test "$(grep -Fc 'mixer_type' "$MPD_CONFIG")" -eq 1
grep -Fq 'mixer_type    "software"' "$MPD_CONFIG"
cat > "$MPD_CONFIG" <<'EOF'
music_directory "/music"
audio_output {
    type "alsa"
    name "camilladsp-loopback"
    device "hw:USB"
    mixer_type "hardware"
}
EOF
ensure_mpd_loopback_mixer || true
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
ensure_mpd_loopback_mixer || true
! grep -Fq 'mixer_type    "software"' "$MPD_CONFIG"
grep -Fq 'mixer_type "hardware"' "$MPD_CONFIG"

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

