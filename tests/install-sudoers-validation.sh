#!/usr/bin/env bash
# Regression: installer validation must check the rule it just wrote, not fail
# because an unrelated sudoers.d file has bad permissions.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

export SUDOERS_RULE="$tmp_dir/etc/sudoers.d/oxide-power"
export OXIDE_INSTALLER_TEST=1
export PATH="$tmp_dir/bin:$PATH"
mkdir -p "$(dirname "$SUDOERS_RULE")" "$tmp_dir/bin"

cat > "$tmp_dir/bin/visudo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[ "${1:-}" = "-cf" ]
[ "${2:-}" = "$SUDOERS_RULE" ]
test -s "$2"
EOF
chmod 755 "$tmp_dir/bin/visudo"

# shellcheck disable=SC1091
source "$repo_root/install.sh"
write_power_sudoers

python3 - "$SUDOERS_RULE" <<'PY'
import os
import stat
import sys

assert stat.S_IMODE(os.stat(sys.argv[1]).st_mode) == 0o440
PY
printf 'installer sudoers validation test passed\n'
