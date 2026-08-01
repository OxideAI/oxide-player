#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

fake_bin="$tmp_dir/bin"
mkdir -p "$fake_bin"

stage="$tmp_dir/oxide-player-arm64"
mkdir -p "$stage/target/release" "$stage/dist"
printf '#!/bin/sh\n' > "$stage/target/release/oxide-player"
chmod +x "$stage/target/release/oxide-player"
printf '<!doctype html>\n' > "$stage/dist/index.html"
tar -czf "$tmp_dir/package.tar.gz" -C "$tmp_dir" oxide-player-arm64

cat > "$fake_bin/uname" <<'EOF'
#!/usr/bin/env bash
if [ "${1:-}" = "-m" ]; then
  printf 'aarch64\n'
else
  printf 'Linux\n'
fi
EOF

cat > "$fake_bin/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

output=""
previous=""
for arg in "$@"; do
  if [ "$previous" = "-o" ]; then
    output="$arg"
    break
  fi
  previous="$arg"
done

count_file="$CURL_STATE"
count="$(cat "$count_file" 2>/dev/null || printf '0')"
count=$((count + 1))
printf '%s' "$count" > "$count_file"

if [ -n "$output" ]; then
  cp "$PACKAGE" "$output"
elif [ "$count" -eq 1 ]; then
  printf '{"tag_name":"oxide-player-v0.12.1","assets":[]}\n'
else
  printf '{"tag_name":"oxide-player-v0.12.1","assets":[{"name":"oxide-player-arm64.tar.gz","browser_download_url":"https://example.invalid/oxide-player-arm64.tar.gz"}]}\n'
fi
EOF
chmod +x "$fake_bin/uname" "$fake_bin/curl"

export PATH="$fake_bin:$PATH"
export CURL_STATE="$tmp_dir/curl-count"
export PACKAGE="$tmp_dir/package.tar.gz"
export REPO_API="https://example.invalid/api"
export BUILD_DIR="$tmp_dir/build"
export RELEASE_ASSET_RETRY_DELAY=0
export OXIDE_INSTALLER_TEST=1

# shellcheck disable=SC1091
source "$repo_root/install.sh"

if ! fetch_release_pkg; then
  printf 'installer did not retry while the release assets were still publishing\n' >&2
  exit 1
fi
test "$(cat "$CURL_STATE")" -ge 2
test -x "$BUILD_DIR/oxide-player-arm64/target/release/oxide-player"
test -f "$BUILD_DIR/oxide-player-arm64/dist/index.html"
printf 'release asset retry test passed\n'
