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
REPO_API="${REPO_API:-https://api.github.com/repos/OxideAI/oxide-player}"
INSTALL_FROM_DIR="${INSTALL_FROM_DIR:-}"          # set to a local checkout to skip cloning
BRANCH="${BRANCH:-main}"

BIN_DIR="${BIN_DIR:-/usr/local/bin}"
SHARE_DIR="${SHARE_DIR:-/usr/local/share/oxide-player}"
CONFIG_DIR="${CONFIG_DIR:-/etc/oxide-player}"
DATA_DIR="${DATA_DIR:-/var/lib/oxide-player}"
CAMILLADSP_CONFIG="${CAMILLADSP_CONFIG:-/etc/camilladsp/config.yml}"
CAMILLADSP_WS="${CAMILLADSP_WS:-ws://127.0.0.1:1234}"
SERVICE_USER="${SERVICE_USER:-oxide}"
# oxide-player binds on port 80 by default so the web UI is reachable at
# http://oxide-player/ or http://oxide-player.local/ without a port number.
# Ports below 1024 need CAP_NET_BIND_SERVICE (set up automatically in the
# systemd unit). Override with LISTEN if you need a different address.
LISTEN="${LISTEN:-0.0.0.0:80}"
# Location of the music library folder. Shared over SMB and served by MPD.
# Override with MPD_MUSIC_DIR if you need a different path.
MUSIC_DIR="${MUSIC_DIR:-${DATA_DIR}/music}"
MPD_MUSIC_DIR="${MPD_MUSIC_DIR:-${MUSIC_DIR}}"
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

detect_arch() {
  # Map the current machine to the release asset suffix used by the packaging
  # workflow: oxide-player-<arch>.tar.gz (arch in {x86_64, arm64}).
  local m
  m="$(uname -m)"
  case "$m" in
    x86_64|amd64)   echo "x86_64" ;;
    aarch64|arm64)  echo "arm64" ;;
    *) die "Unsupported architecture '$m' for prebuilt packages. Install from source instead (set INSTALL_FROM_DIR, or build manually)." ;;
  esac
}

fetch_release_pkg() {
  # Download the latest GitHub release package for this architecture and unpack
  # it into BUILD_DIR so build_backend() can reuse the prebuilt binary via the
  # INSTALL_FROM_DIR path. Falls back to a source clone on any failure.
  command -v curl >/dev/null 2>&1 || { warn "curl missing — skipping release download."; return 1; }
  command -v jq   >/dev/null 2>&1 || { warn "jq missing — skipping release download."; return 1; }

  local arch asset url tag
  arch="$(detect_arch)"
  asset="oxide-player-${arch}.tar.gz"

  log "Looking up latest release asset: $asset"
  url="$(curl -fsSL "$REPO_API/releases/latest" \
        | jq -r --arg a "$asset" '.tag_name as $t | (.assets[]? | select(.name==$a) | {url: .browser_download_url, tag: $t}) | "\(.tag)\t\(.url)"' \
        | head -1)"

  if [ -z "$url" ]; then
    warn "No matching release asset found — will build from source."
    return 1
  fi
  tag="${url%%$'\t'*}"
  url="${url#*$'\t'}"
  log "Found $asset in release $tag"

  rm -rf "$BUILD_DIR"
  mkdir -p "$BUILD_DIR"
  run curl -fsSL -L "$url" -o "$BUILD_DIR/$asset"
  run tar -xzf "$BUILD_DIR/$asset" -C "$BUILD_DIR"
  SRC_DIR="$BUILD_DIR/oxide-player-${arch}"
  if [ ! -d "$SRC_DIR" ]; then
    warn "Unexpected package layout — will build from source."
    return 1
  fi
  log "Using prebuilt release package: $SRC_DIR"
  return 0
}

fetch_source() {
  if [ -n "$INSTALL_FROM_DIR" ]; then
    SRC_DIR="$INSTALL_FROM_DIR"
    log "Using local source: $SRC_DIR"
    return
  fi
  if fetch_release_pkg; then
    return
  fi
  SRC_DIR="$BUILD_DIR"
  rm -rf "$SRC_DIR"
  run git clone --depth 1 --branch "$BRANCH" "$REPO_URL" "$SRC_DIR"
}

build_backend() {
  # When installing from a local checkout or a prebuilt release package that
  # already carries a release binary, skip the toolchain entirely and copy the
  # existing artifact instead of compiling on-device.
  if [ -x "$SRC_DIR/target/release/oxide-player" ]; then
    log "Prebuilt backend found in $SRC_DIR — skipping cargo build"
    run install -Dm0755 "$SRC_DIR/target/release/oxide-player" "$BIN_DIR/oxide-player"
    log "Installed backend -> $BIN_DIR/oxide-player"
    return
  fi
  log "Building backend (release)..."
  ensure_rust
  # backend/ is a workspace member, so cargo places the binary in the
  # workspace root target/ dir, not backend/target/.
  ( cd "$SRC_DIR" && cargo build --release )
  run install -Dm0755 "$SRC_DIR/target/release/oxide-player" "$BIN_DIR/oxide-player"
  log "Installed backend -> $BIN_DIR/oxide-player"
}

build_frontend() {
  # A prebuilt release package ships a ready-made dist/ at its root. Reuse it
  # instead of compiling the frontend (no node toolchain needed on-device).
  if [ -d "$SRC_DIR/dist" ] && [ -f "$SRC_DIR/dist/index.html" ]; then
    log "Using prebuilt frontend dist from release package"
    run mkdir -p "$SHARE_DIR/dist"
    run cp -r "$SRC_DIR/dist/." "$SHARE_DIR/dist/"
    log "Installed frontend UI -> $SHARE_DIR/dist"
    return
  fi
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

  # Add the service user to the group that owns the music/library directory, so
  # the scanner can traverse and read the files. Without this, a library dir
  # inside another user's home (e.g. /home/you/music with mode 750) would be
  # inaccessible to the service user, resulting in "scanned: 0".
  for _libdir in "$MPD_MUSIC_DIR"; do
    if [ -d "$_libdir" ]; then
      _grp="$(stat -c '%G' "$_libdir" 2>/dev/null || true)"
      if [ -n "$_grp" ] && [ "$_grp" != "root" ] && ! groups "$SERVICE_USER" | tr ' ' '\n' | grep -qxF "$_grp"; then
        run usermod -aG "$_grp" "$SERVICE_USER"
      fi
    fi
  done

  # Ensure music directory exists (will be shared over SMB)
  run mkdir -p "$MPD_MUSIC_DIR"

  run mkdir -p "$DATA_DIR/covers" "$CONFIG_DIR" "$(dirname "$CAMILLADSP_CONFIG")"
  run chown -R "$SERVICE_USER:$SERVICE_USER" "$DATA_DIR" "$CONFIG_DIR" "$(dirname "$CAMILLADSP_CONFIG")"
  run chown -R "$SERVICE_USER:$SERVICE_USER" "$MPD_MUSIC_DIR"
  run chmod 755 "$DATA_DIR"
}

setup_hostname() {
  local hostname="oxide-player"
  local current
  current="$(hostname -s 2>/dev/null || echo '')"
  if [ "$current" != "$hostname" ]; then
    log "Setting hostname to $hostname"
    run hostnamectl set-hostname "$hostname"
    # Ensure /etc/hosts has the new hostname mapped to 127.0.0.1
    # so tools like \`hostname -s\` and Avahi resolve correctly.
    if grep -qi "$hostname" /etc/hosts 2>/dev/null; then
      sed -i "s/\\b$current\\b/$hostname/gi" /etc/hosts 2>/dev/null || true
    else
      sed -i "s/^127\\.0\\.0\\.1.*/& $hostname/" /etc/hosts 2>/dev/null || true
    fi
    log "Hostname set to $hostname — accessible as http://${hostname}.local/"
  else
    log "Hostname already $hostname"
  fi
}

setup_samba() {
  local share_name="Music"
  local share_path="$MPD_MUSIC_DIR"

  apt_install samba

  # Backup existing smb.conf
  if [ -f /etc/samba/smb.conf ] && [ ! -f /etc/samba/smb.conf.pre-oxide ]; then
    run cp /etc/samba/smb.conf /etc/samba/smb.conf.pre-oxide
    warn "Backed up existing smb.conf to smb.conf.pre-oxide"
  fi

  log "Writing Samba config"
  cat > /etc/samba/smb.conf <<SAMBAEOF
[global]
   workgroup = WORKGROUP
   server string = oxide-player
   netbios name = OXIDE-PLAYER
   security = user
   map to guest = Bad User
   guest account = nobody

[Music]
   path = $share_path
   browseable = yes
   read only = no
   guest ok = yes
   force user = $SERVICE_USER
   force group = $SERVICE_USER
   create mask = 0644
   directory mask = 0755
SAMBAEOF

  # Ensure the music dir exists with correct ownership
  run mkdir -p "$share_path"
  run chown -R "$SERVICE_USER:$SERVICE_USER" "$share_path"
  run chmod 755 "$share_path"

  # Enable and restart smbd
  run systemctl enable smbd || true
  run systemctl restart smbd || warn "smbd restart failed — check samba config"
  log "Samba share 'Music' → $share_path (guest writable, force-user=$SERVICE_USER)"
}

setup_avahi() {
  apt_install avahi-daemon

  # Ensure avahi runs
  run systemctl enable avahi-daemon || true

  log "Installing Avahi service definitions"
  mkdir -p /etc/avahi/services

  # Advertise the oxide-player web UI on the LAN via mDNS/Bonjour
  cat > /etc/avahi/services/oxide-player.service <<AVAHIEOF
<?xml version="1.0" standalone='no'?>
<!DOCTYPE service-group SYSTEM "avahi-service.dtd">
<service-group>
  <name>Oxide Player on %h</name>
  <service>
    <type>_http._tcp</type>
    <port>${LISTEN##*:}</port>
    <txt-record>path=/</txt-record>
  </service>
  <service>
    <type>_musicplayer._sub._http._tcp</type>
    <port>${LISTEN##*:}</port>
  </service>
  <service>
    <type>_smb._tcp</type>
    <port>445</port>
  </service>
</service-group>
AVAHIEOF

  # Also advertise the Music share via Samba's native mDNS
  # by ensuring samba registers its shares via Avahi
  mkdir -p /etc/avahi/services

  run systemctl restart avahi-daemon || warn "avahi-daemon restart failed"
  log "Avahi services registered — reachable as http://oxide-player.local:${LISTEN##*:}/"
}

setup_asound() {
  # Detect the default audio output device (skip the snd-aloop loopback)
  # and write /etc/asound.conf so ALSA's "default" device points to the
  # first real playback card found.
  #
  # When no hardware is detected (headless / no sound driver) the file is
  # skipped and CamillaDSP falls back to hw:DAC (user must configure).
  local conf=/etc/asound.conf

  if ! command -v aplay >/dev/null 2>&1; then
    warn "aplay not found — skipping ALSA default config."
    warn "Install alsa-utils and run 'aplay -l' to find your device."
    return
  fi

  # Parse aplay -l, find first non-Loopback playback card
  local card="" dev="" name=""
  while IFS= read -r line; do
    if [[ $line =~ ^card[[:space:]]+([0-9]+):[[:space:]]([A-Za-z].+),[[:space:]]+device[[:space:]]+([0-9]+):[[:space:]](.+) ]]; then
      local c="${BASH_REMATCH[1]}"
      local cname="${BASH_REMATCH[2]}"
      local d="${BASH_REMATCH[3]}"
      local dname="${BASH_REMATCH[4]}"
      # skip the loopback module
      if [[ $cname != *Loopback* ]] && [[ $cname != *loopback* ]]; then
        card="$c"
        dev="$d"
        name="$cname — $dname"
        break
      fi
    fi
  done < <(aplay -l 2>/dev/null || true)

  if [ -z "$card" ]; then
    warn "No audio hardware detected (only loopback found)."
    warn "CamillaDSP will use 'hw:DAC'. Edit /etc/asound.conf manually."
    return
  fi

  log "Detected audio output: card $card ($name)"
  log "Writing ALSA default config ($conf)"
  cat > "$conf" <<ASOUNDEOF
# oxide-player: default audio output → card $card, device $dev
# Auto-detected during install. Edit this file to change the default.
#
# To list available devices:
#   aplay -l
#
# Override per-session via the ALSA_PCM environment variable:
#   ALSA_PCM=hw:1,0  mplayer track.flac

pcm.!default {
    type hw
    card $card
    device $dev
}

ctl.!default {
    type hw
    card $card
}
ASOUNDEOF
  log "ALSA default → card $card, device $dev"
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
    type: Alsa
    channels: 2
    device: hw:Loopback,1
    format: S32_LE
  playback:
    type: Alsa
    channels: 2
    device: default
    format: S32_LE
  samplerate: 44100
  chunksize: 1024
  queuelimit: 4
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
  "mpd_music_directory": "$MPD_MUSIC_DIR",
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
  local _cam_host="${CAMILLADSP_WS#ws://}"
  _cam_host="${_cam_host%:*}"
  local _cam_port="${CAMILLADSP_WS##*:}"
  cat > /etc/systemd/system/camilladsp.service <<EOF
[Unit]
Description=CamillaDSP audio processor
After=network-online.target sound.target
Wants=sound.target

[Service]
Type=simple
User=$SERVICE_USER
ExecStart=$BIN_DIR/camilladsp $CAMILLADSP_CONFIG -a $_cam_host -p $_cam_port
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
# Allow binding to port 80 (privileged port) as non-root user
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE

[Install]
WantedBy=multi-user.target
EOF
  fi

  run mkdir -p /etc/modules-load.d
  echo "snd-aloop" > /etc/modules-load.d/oxide.conf
  run systemctl daemon-reload
  run systemctl enable mpd camilladsp oxide-player
  run systemctl restart mpd camilladsp oxide-player
}

finish() {
  local _ip="$(hostname -I | awk '{print $1}')"
  local _hostname="$(hostname -s | tr '[:upper:]' '[:lower:]')"
  local _port="${LISTEN##*:}"
  local _port_suffix=""
  [ "$_port" != "80" ] && _port_suffix=":$_port"
  log "Done."
  log "Web UI:        http://$_ip${_port_suffix}/"
  log "  mDNS:        http://${_hostname}.local${_port_suffix}/"
  log "Kiosk view:    http://$_ip${_port_suffix}/kiosk"
  log "Music share:   smb://$_ip/Music"
  log "  mDNS:        smb://${_hostname}.local/Music"
  log "Edit config:   $CONFIG_DIR/config.json"
  log "Logs:          journalctl -u oxide-player -f"
  log ""
  log "Copy your music files into the shared Music folder, then scan in the web UI:"
  log "  Settings → Music library sources → Rescan library"
}

check_linux() {
  case "$(uname -s)" in
    Linux) ;;
    *) die "oxide-player only runs on Linux (you're on $(uname -s))." ;;
  esac
}

check_dependencies() {
  # These must already be present; the installer relies on them at runtime.
  local missing=()
  for c in mpd camilladsp; do
    command -v "$c" >/dev/null 2>&1 || missing+=("$c")
  done
  if [ "${#missing[@]}" -gt 0 ]; then
    warn "Missing runtime dependency/ies: ${missing[*]}"
    warn "The installer will try to provide them, but ensure they are available:"
    warn "  - mpd:          apt-get install -y mpd"
    warn "  - camilladsp:   built from source by this installer (needs Rust)"
  fi
}

main() {
  need_root
  check_linux
  detect_os
  apt_install curl jq git build-essential pkg-config libssl-dev \
              libasound2-dev alsa-utils mpd mpc ffmpeg samba avahi-daemon
  check_dependencies
  ensure_camilladsp
  fetch_source
  build_backend
  build_frontend
  setup_user_dirs
  setup_asound
  setup_samba
  setup_avahi
  write_mpd_config
  write_camilladsp_config
  write_oxide_config
  install_units
  finish
}

main "$@"
