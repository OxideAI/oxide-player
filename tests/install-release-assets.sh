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

if [ -n "${output:-}" ]; then
  cp "$PACKAGE" "$output"
elif [ "${NO_ASSET:-0}" = "1" ]; then
  printf '{"tag_name":"oxide-player-v0.12.1","assets":[]}\n'
elif [ "$count" -eq 1 ]; then
  printf '{"tag_name":"oxide-player-v0.12.1","assets":[]}\n'
else
  printf '{"tag_name":"oxide-player-v0.12.1","assets":[{"name":"oxide-player-arm64.tar.gz","browser_download_url":"https://example.invalid/oxide-player-arm64.tar.gz"}]}\n'
fi
EOF
chmod +x "$fake_bin/uname" "$fake_bin/curl"

cat > "$fake_bin/git" <<'EOF'
#!/usr/bin/env bash
printf 'called\n' >> "$GIT_STATE"
exit 0
EOF
cat > "$fake_bin/cargo" <<'EOF'
#!/usr/bin/env bash
printf 'called\n' >> "$CARGO_STATE"
exit 0
EOF
chmod +x "$fake_bin/git" "$fake_bin/cargo"

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
export BIN_DIR="$tmp_dir/installed-bin"
install() {
  local source="${@: -2:1}" destination="${@: -1}"
  mkdir -p "$(dirname "$destination")"
  cp "$source" "$destination"
}

export CARGO_STATE="$tmp_dir/cargo-called"
smoke_backend() { return 0; }
build_backend
cmp "$BUILD_DIR/oxide-player-arm64/target/release/oxide-player" "$BIN_DIR/oxide-player"
test ! -e "$CARGO_STATE"
release_only_update=true
smoke_backend() { return 1; }
if ( install_prebuilt_backend "$BUILD_DIR/oxide-player-arm64/target/release/oxide-player" ); then
  printf 'update continued after a failing release-binary smoke test\n' >&2
  exit 1
fi
test ! -e "$CARGO_STATE"


export GIT_STATE="$tmp_dir/git-called"
export CURL_STATE="$tmp_dir/no-asset-curl-count"
export NO_ASSET=1
export RELEASE_ASSET_RETRIES=1
declare -F fetch_update_source >/dev/null || {
  printf 'installer has no release-only update path\n' >&2
  exit 1
}
if ( fetch_update_source ); then
  printf 'update accepted a missing release asset and may compile from source\n' >&2
  exit 1
fi
test ! -e "$GIT_STATE"
printf 'release-only update test passed\n'
printf 'release asset retry test passed\n'
