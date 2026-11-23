#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
task_dir="$(mktemp -d)"
trap 'rm -rf "$task_dir"' EXIT
export OXIDE_INSTALLER_TEST=1 CONFIG_DIR="$task_dir/config" DATA_DIR="$task_dir/data"
export SAMBA_CONFIG="$task_dir/smb.conf" SAMBA_SHARES_CONFIG="$task_dir/smb-shares.conf"
export MPD_MUSIC_DIR="$task_dir/music" SERVICE_USER=oxide-test
mkdir -p "$CONFIG_DIR" "$DATA_DIR" "$MPD_MUSIC_DIR" "$task_dir/USB disk" "$task_dir/NAS"
source "$repo_root/install.sh"
apt_install() { :; }
systemctl() { :; }
chown() { :; }
chmod() { :; }
printf '{"library_dirs":["%s/USB disk","%s/NAS"]}\n' "$task_dir" "$task_dir" > "$CONFIG_DIR/config.json"
setup_samba > "$task_dir/log" 2>&1
python3 - "$SAMBA_SHARES_CONFIG" "$task_dir" <<'PY'
import sys
from pathlib import Path
text=Path(sys.argv[1]).read_text()
assert '[Oxide-USB_disk]' in text and '[Oxide-NAS]' in text, text
assert '[Music]' not in text, 'deleted default source was restored'
assert sys.argv[2]+'/USB disk' in text and sys.argv[2]+'/NAS' in text
PY
cp "$SAMBA_SHARES_CONFIG" "$task_dir/expected"
ensure_samba_shares > "$task_dir/log" 2>&1
cmp "$SAMBA_SHARES_CONFIG" "$task_dir/expected"
printf '{"library_dirs":[]}\n' > "$CONFIG_DIR/config.json"
setup_samba > "$task_dir/log" 2>&1
test "$(grep -c '^\[' "$SAMBA_SHARES_CONFIG" || true)" -eq 0
printf 'Samba setup/update preserves configured sources and empty libraries\n'
