#!/usr/bin/env bash
#
# oxide-player installer
#
#   curl -fsSL https://raw.githubusercontent.com/OxideAI/oxide-player/main/install.sh | sudo bash
#
# Idempotent. Targets Debian-based systems (Raspberry Pi OS, Ubuntu, Debian).
# All paths/URLs below can be overridden via environment variables.
set -euo pipefail

# ---- configurable knobs -----------------------------------------------------
REPO_URL="${REPO_URL:-https://github.com/OxideAI/oxide-player.git}"
INSTALL_FROM_DIR="${INSTALL_FROM_DIR:-}"          # set to a local checkout to skip cloning
BRANCH="${BRANCH:-main}"

BIN_DIR="${BIN_DIR:-/usr/local/bin}"
SHARE_DIR="${SHARE_DIR:-/usr/local/share/oxide-player}"
CONFIG_DIR="${CONFIG_DIR:-/etc/oxide-player}"
DATA_DIR="${DATA_DIR:-/var/lib/oxide-player}"
CAMILLADSP_CONFIG="${CAMILLADSP_CONFIG:-/etc/camilladsp/config.yml}"
CAMILLADSP_WS="${CAMILLADSP_WS:-ws://127.0.0.1:1234}"
SERVICE_USER="${SERVICE_USER:-oxide}"
LISTEN="${LISTEN:-0.0.0.0:8000}"
MPD_MUSIC_DIR="${MPD_MUSIC_DIR:-/var/lib/mpd/music}"
BUILD_DIR="${BUILD_DIR:-/tmp/oxide-player-build}"

# ---- helpers ----------------------------------------------------------------
log()  { printf '\033[1;34m[oxide]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[oxide]\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m[oxide]\033[0m %s\n' "$*" >&2; exit 1; }

run() { log "+ $*"; "$@"; }

need_root() {
  [ "$(id -u)" -eq 0 ] || die "Run as root (e.g. sudo bash)."
}

detect_os() {
  if [ -r /etc/os-release ]; then
    . /etc/os-release
    log "Detected: $PRETTY_NAME"
    case "${ID:-},${ID_LIKE:-}" in
      *debian*|*ubuntu*|*raspbian*) ;;
      *) warn "Unsupported OS; continuing but you may need to adjust package names." ;;
    esac
  else
    warn "/etc/os-release not found; assuming a Debian-like system."
  fi
}

apt_install() {
  local missing=()
  for p in "$@"; do
    dpkg -s "$p" >/dev/null 2>&1 || missing+=("$p")
  done
  if [ "${#missing[@]}" -gt 0 ]; then
    run apt-get update
    run apt-get install -y "${missing[@]}"
  fi
}

ensure_rust() {
  if command -v cargo >/dev/null 2>&1; then
    log "Rust toolchain already present: $(cargo --version)"
    return
  fi
  log "Installing Rust toolchain via rustup..."
  run curl -fsSL https://sh.rustup.rs -o /tmp/rustup.sh
  run sh /tmp/rustup.sh -y --profile minimal
  rm -f /tmp/rustup.sh
  # shellcheck disable=SC1091
  [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
  export PATH="$HOME/.cargo/bin:$PATH"
}

ensure_camilladsp() {
  if command -v camilladsp >/dev/null 2>&1; then
    log "camilladsp present: $(camilladsp --version 2>&1 | head -1)"
    return
  fi
  ensure_rust
  # camilladsp is not published to crates.io; build from the official repo,
  # pinned to a release tag for reproducible installs.
  log "Installing camilladsp (pinned to v4.1.3)..."
  run cargo install --git https://github.com/HEnquist/camilladsp --tag v4.1.3
}

fetch_source() {
  if [ -n "$INSTALL_FROM_DIR" ]; then
    SRC_DIR="$INSTALL_FROM_DIR"
    log "Using local source: $SRC_DIR"
    return
  fi
  SRC_DIR="$BUILD_DIR"
  rm -rf "$SRC_DIR"
  run git clone --depth 1 --branch "$BRANCH" "$REPO_URL" "$SRC_DIR"
}

build_backend() {
  log "Building backend (release)..."
  ( cd "$SRC_DIR/backend" && cargo build --release )
  run install -Dm0755 "$SRC_DIR/backend/target/release/oxide-player" "$BIN_DIR/oxide-player"
  log "Installed backend -> $BIN_DIR/oxide-player"
}

build_frontend() {
  if ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1; then
    warn "node/npm not found — skipping web UI build."
    warn "Install the frontend manually: copy a built 'dist/' into $SHARE_DIR/dist"
    return
  fi
  local nodever
  nodever="$(node -v | tr -d 'v' | cut -d. -f1)"
  if [ "${nodever:-0}" -lt 18 ]; then
    warn "node < 18 (found $(node -v)) — skipping web UI build."
    return
  fi
  log "Building frontend..."
  ( cd "$SRC_DIR/frontend" && npm ci && npm run build )
  run mkdir -p "$SHARE_DIR/dist"
  run cp -r "$SRC_DIR/frontend/dist/." "$SHARE_DIR/dist/"
  log "Installed frontend UI -> $SHARE_DIR/dist"
}

setup_user_dirs() {
  if ! id "$SERVICE_USER" >/dev/null 2>&1; then
    run useradd --system --no-create-home --shell /usr/sbin/nologin "$SERVICE_USER"
  fi
  run usermod -aG audio "$SERVICE_USER" || true
  run mkdir -p "$DATA_DIR/covers" "$CONFIG_DIR" "$(dirname "$CAMILLADSP_CONFIG")"
  run chown -R "$SERVICE_USER:$SERVICE_USER" "$DATA_DIR" "$CONFIG_DIR" "$(dirname "$CAMILLADSP_CONFIG")"
  run chmod 755 "$DATA_DIR"
}

write_mpd_config() {
  local conf=/etc/mpd.conf
  if [ -f "$conf" ] && [ ! -f "$conf.pre-oxide" ]; then
    run cp "$conf" "$conf.pre-oxide"
    warn "Backed up existing $conf to $conf.pre-oxide"
  fi
  log "Writing MPD config ($conf)"
  cat > "$conf" <<EOF
music_directory     "$MPD_MUSIC_DIR"
playlist_directory  "$DATA_DIR/playlists"
db_file             "$DATA_DIR/mpd.db"
log_file            "$DATA_DIR/mpd.log"
pid_file            "$DATA_DIR/mpd.pid"
state_file          "$DATA_DIR/mpd.state"
bind_to_address     "127.0.0.1"
port                "6600"
auto_update         "yes"

audio_output {
    type          "alsa"
    name          "camilladsp-loopback"
    device        "hw:Loopback,0"
    dop          "no"
}
EOF
  run mkdir -p "$DATA_DIR/playlists"
  run chown -R "$SERVICE_USER:$SERVICE_USER" "$DATA_DIR" 2>/dev/null || true
}

write_camilladsp_config() {
  log "Writing CamillaDSP config ($CAMILLADSP_CONFIG)"
  cat > "$CAMILLADSP_CONFIG" <<'EOF'
devices:
  capture:
    type: Raw
    channels: 2
    device: hw:Loopback,1
    format: S32LE
  playback:
    type: Raw
    channels: 2
    device: hw:DAC
    format: S32LE
samplerate: 44100
channels: 2
mixers: {}
filters: {}
pipeline: []
EOF
  run chown "$SERVICE_USER:$SERVICE_USER" "$CAMILLADSP_CONFIG"
}

write_oxide_config() {
  log "Writing oxide-player config ($CONFIG_DIR/config.json)"
  local dirs_json=""
  for d in "$MPD_MUSIC_DIR"; do
    dirs_json+="\"$d\","
  done
  dirs_json="[${dirs_json%,}]"
  cat > "$CONFIG_DIR/config.json" <<EOF
{
  "mpd_host": "127.0.0.1",
  "mpd_port": 6600,
  "listen": "$LISTEN",
  "data_dir": "$DATA_DIR",
  "library_dirs": $dirs_json,
  "static_dir": "$SHARE_DIR/dist",
  "camilladsp_config_path": "$CAMILLADSP_CONFIG",
  "camilladsp_ws_url": "$CAMILLADSP_WS",
  "default_dsp_profiles": []
}
EOF
  run chown "$SERVICE_USER:$SERVICE_USER" "$CONFIG_DIR/config.json"
}

install_units() {
  log "Installing systemd units"
  cat > /etc/systemd/system/camilladsp.service <<EOF
[Unit]
Description=CamillaDSP audio processor
After=network-online.target sound.target
Wants=sound.target

[Service]
Type=simple
User=$SERVICE_USER
ExecStart=$BIN_DIR/camilladsp --config $CAMILLADSP_CONFIG --wsurl $CAMILLADSP_WS
Restart=on-failure

[Install]
WantedBy=multi-user.target
EOF

  if [ -f "$SRC_DIR/contrib/systemd/oxide-player.service" ]; then
    run install -Dm0644 "$SRC_DIR/contrib/systemd/oxide-player.service" /etc/systemd/system/oxide-player.service
  else
    cat > /etc/systemd/system/oxide-player.service <<EOF
[Unit]
Description=oxide-player — audiophile MPD + CamillaDSP controller
After=network-online.target mpd.service camilladsp.service
Wants=mpd.service camilladsp.service

[Service]
Type=simple
User=$SERVICE_USER
WorkingDirectory=$DATA_DIR
ExecStart=$BIN_DIR/oxide-player -c $CONFIG_DIR/config.json
Nice=-5
Restart=on-failure
RestartSec=3

[Install]
WantedBy=multi-user.target
EOF
  fi

  echo "snd-aloop" > /etc/modules-load.d/oxide.conf
  run systemctl daemon-reload
  run systemctl enable mpd camilladsp oxide-player
  run systemctl restart mpd camilladsp oxide-player
}

finish() {
  log "Done."
  log "Web UI:        http://$(hostname -I | awk '{print $1}'):${LISTEN##*:}/"
  log "Kiosk view:    http://$(hostname -I | awk '{print $1}'):${LISTEN##*:}/kiosk"
  log "Edit config:   $CONFIG_DIR/config.json"
  log "Logs:          journalctl -u oxide-player -f"
}

main() {
  need_root
  detect_os
  apt_install curl jq git build-essential pkg-config libssl-dev \
              libasound2-dev alsa-utils mpd ffmpeg
  ensure_camilladsp
  fetch_source
  build_backend
  build_frontend
  setup_user_dirs
  write_mpd_config
  write_camilladsp_config
  write_oxide_config
  install_units
  finish
}

main "$@"
