#!/usr/bin/env bash
# Permission changes on mounted music libraries must not abort installation.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
child=false
if [ "${1:-}" = "--run-install" ]; then
  child=true
  tmp_dir="$2"
  failure="${3:-}"
  set --
else
  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' EXIT
fi
calls="$tmp_dir/calls"

export OXIDE_INSTALLER_TEST=1
export CONFIG_DIR="$tmp_dir/etc/oxide-player"
export DATA_DIR="$tmp_dir/var/lib/oxide-player"
export MPD_MUSIC_DIR="$DATA_DIR/music"
export CAMILLADSP_CONFIG="$tmp_dir/etc/camilladsp/config.yml"
export SAMBA_CONFIG="$tmp_dir/etc/samba/smb.conf"
export SAMBA_SHARES_CONFIG="$DATA_DIR/smb-shares.conf"
export SERVICE_USER="oxide-test"

# shellcheck disable=SC1091
source "$repo_root/install.sh"

# Keep the real install sequence and directory/Samba setup, but avoid packages,
# services, mount changes, and other system integration in this test.
for step in need_root check_linux detect_os apt_install check_dependencies \
  ensure_camilladsp fetch_source build_backend build_frontend \
  fix_vfat_music_mounts setup_asound setup_avahi setup_bluetooth \
  setup_wifi_powersave_off setup_wifi_heal setup_wired_priority setup_sata_lpm \
  setup_music_bind write_mpd_config setup_mpd_systemd write_camilladsp_config \
  write_visualizer_fifo write_oxide_config setup_airplay install_units \
  write_motd write_kiosk sync_rtc_from_system; do
  eval "$step() { :; }"
done
id() { return 0; }
stat() { printf 'music-group\n'; }
groups() { printf '%s : audio\n' "$SERVICE_USER"; }
findmnt() { return 1; }
systemctl() { printf 'service %s\n' "$*" >> "$calls"; }
finish() { printf 'install complete\n' >> "$calls"; }

permission_change() {
  printf '%s\n' "$*" >> "$calls"
  if [ "$failure" = "all" ] || [ "$failure" = "$*" ]; then
    printf 'Operation not permitted: %s\n' "$*" >&2
    return 1
  fi
}
chown() { permission_change chown "$@"; }
chmod() { permission_change chmod "$@"; }
usermod() { permission_change usermod "$@"; }

if $child; then
  if [ "$failure" = "mkdir" ]; then
    mkdir() { return 1; }
  fi
  main
  exit
fi

failures=(
  "chown -R $SERVICE_USER:$SERVICE_USER $DATA_DIR $CONFIG_DIR $(dirname "$CAMILLADSP_CONFIG")"
  "chown -R $SERVICE_USER:$SERVICE_USER $MPD_MUSIC_DIR"
  "chmod -R u+rwX,go+rX $MPD_MUSIC_DIR"
  "chmod 755 $DATA_DIR"
  "usermod -aG audio $SERVICE_USER"
  "usermod -aG music-group $SERVICE_USER"
  "chown $SERVICE_USER:$SERVICE_USER $SAMBA_SHARES_CONFIG"
  "chmod 755 $MPD_MUSIC_DIR"
  "all"
  ""
)

for failure in "${failures[@]}"; do
  : > "$calls"
  mkdir -p "$MPD_MUSIC_DIR" "$(dirname "$SAMBA_CONFIG")"
  # Run as a standalone command under errexit: invoking main in an if/||
  # condition would disable errexit inside it and hide this regression.
  status=0
  bash "${BASH_SOURCE[0]}" --run-install "$tmp_dir" "$failure" \
    > "$tmp_dir/output" 2>&1 || status=$?
  if [ "$status" -ne 0 ]; then
    printf 'FAIL: installation aborted after %s (exit %s)\n' "$failure" "$status" >&2
    cat "$tmp_dir/output" >&2
    exit 1
  fi
  grep -Fqx 'install complete' "$calls"
  grep -Fqx 'service restart smbd' "$calls"
  grep -Fqx "chmod 755 $MPD_MUSIC_DIR" "$calls"
  test -s "$SAMBA_SHARES_CONFIG"
  if [ -n "$failure" ]; then
    grep -Fq 'continuing' "$tmp_dir/output"
  else
    ! grep -Fq 'continuing' "$tmp_dir/output"
  fi
done

# Real setup errors must still abort the install.
: > "$calls"
status=0
bash "${BASH_SOURCE[0]}" --run-install "$tmp_dir" mkdir \
  > "$tmp_dir/output" 2>&1 || status=$?
test "$status" -ne 0
! grep -Fqx 'install complete' "$calls"

printf 'installer permission failure tests passed\n'
