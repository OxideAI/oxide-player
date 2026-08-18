#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

fake_bin="$tmp_dir/bin"
mkdir -p "$fake_bin" "$tmp_dir/home"

cat > "$fake_bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [ "${1:-}" = "--version" ]; then
  printf 'cargo 1.90.0\n'
  exit 0
fi

if [ "${1:-}" = "install" ]; then
  mkdir -p "$HOME/.cargo/bin"
  printf '#!/bin/sh\n' > "$HOME/.cargo/bin/camilladsp"
  chmod 755 "$HOME/.cargo/bin/camilladsp"
  exit 0
fi

printf 'unexpected cargo invocation: %s\n' "$*" >&2
exit 1
EOF
chmod 755 "$fake_bin/cargo"

export PATH="$fake_bin:$PATH"
export HOME="$tmp_dir/home"
export BIN_DIR="$tmp_dir/installed"
export OXIDE_INSTALLER_TEST=1

# shellcheck disable=SC1091
source "$repo_root/install.sh"
ensure_camilladsp

test -x "$BIN_DIR/camilladsp"
printf 'camilladsp binary installation test passed\n'
