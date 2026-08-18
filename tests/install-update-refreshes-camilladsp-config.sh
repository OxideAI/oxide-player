#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

export OXIDE_INSTALLER_TEST=1
export CAMILLADSP_CONFIG="$tmp_dir/camilladsp/config.yml"
# shellcheck disable=SC1091
source "$repo_root/install.sh"

called=0
need_root() { :; }
apt_install() { :; }
ensure_camilladsp() { :; }
fetch_source() { :; }
build_backend() { :; }
build_frontend() { :; }
ensure_mpd_include() { :; }
ensure_mpd_loopback_mixer() { :; }
ensure_asound_loopback() { :; }
ensure_samba_shares() { :; }
write_visualizer_fifo() { :; }
write_camilladsp_config() { called=1; }
write_oxide_config() { :; }
setup_bluetooth() { :; }
setup_airplay() { :; }
install_units() { :; }
write_motd() { :; }
finish() { :; }

do_update

test "$called" -eq 1
printf 'update CamillaDSP config refresh test passed\n'
