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
MPD_CONFIG="${MPD_CONFIG:-/etc/mpd.conf}"
CAMILLADSP_CONFIG="${CAMILLADSP_CONFIG:-/etc/camilladsp/config.yml}"
CAMILLADSP_WS="${CAMILLADSP_WS:-ws://127.0.0.1:1234}"
AIRPLAY_NAME="${AIRPLAY_NAME:-Oxide Player}"
AIRPLAY_CONFIG="${AIRPLAY_CONFIG:-${CONFIG_DIR}/shairport-sync.conf}"
ASOUND_CONFIG="${ASOUND_CONFIG:-/etc/asound.conf}"
SAMBA_CONFIG="${SAMBA_CONFIG:-/etc/samba/smb.conf}"
SAMBA_SHARES_CONFIG="${SAMBA_SHARES_CONFIG:-${DATA_DIR}/smb-shares.conf}"
SERVICE_USER="${SERVICE_USER:-oxide}"
SYSTEMD_DIR="${SYSTEMD_DIR:-/etc/systemd/system}"
SUDOERS_RULE="${SUDOERS_RULE:-/etc/sudoers.d/oxide-power}"
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
# Release assets are uploaded after the GitHub release is published. Keep the
# installer from racing that upload on a fresh release.
RELEASE_ASSET_RETRIES="${RELEASE_ASSET_RETRIES:-30}"
RELEASE_ASSET_RETRY_DELAY="${RELEASE_ASSET_RETRY_DELAY:-10}"
# Wall display kiosk (cage + Chromium on the HDMI panel). Opt-in: disabled by
# default so headless installs keep the login TTY and avoid burning the GPU/DRM
# on boxes without a panel; set to 1 to provision it. KIOSK_IDLE_SECONDS and
# KIOSK_TTY tune the blank timer and VT when enabled.
KIOSK_ENABLED="${KIOSK_ENABLED:-0}"

# ---- CLI / mode ------------------------------------------------------------
update_mode=false
fix_perms_mode=false
uninstall_mode=false

show_usage() {
  cat <<'EOF'
Usage: sudo bash install.sh [OPTIONS]

Install, update, or uninstall oxide-player on a Debian-based system.

Options:
  --update     Replace binary + frontend, install/repair phone audio
               receivers, and repair the managed MPD include.
  --uninstall  Remove Oxide binaries, services, and system integration.
               Preserves config, library, playlists, and runtime data.
  --fix-perms  Repair music library ownership/permissions so the service
               user and MPD can read it (fixes a silently empty library
               after music was copied with root ownership, e.g. sudo).
  --help, -h   Show this help.

Environment variables (all optional):
  DATA_DIR, MPD_CONFIG, LISTEN, MUSIC_DIR, MPD_MUSIC_DIR, CAMILLADSP_CONFIG,
  CAMILLADSP_WS, AIRPLAY_NAME, AIRPLAY_CONFIG, ASOUND_CONFIG, SAMBA_CONFIG,
  SAMBA_SHARES_CONFIG, SERVICE_USER, BUILD_DIR, RELEASE_ASSET_RETRIES,
  RELEASE_ASSET_RETRY_DELAY, KIOSK_ENABLED, KIOSK_IDLE_SECONDS, KIOSK_TTY
EOF
  exit 0
}

while [ $# -gt 0 ]; do
  case "${1:-}" in
    --update) update_mode=true ;;
    --uninstall) uninstall_mode=true ;;
    --fix-perms) fix_perms_mode=true ;;
    --help|-h) show_usage ;;
    *) warn "Unknown option: $1 (try --help)" ;;
  esac
  shift
done

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
  local installed="$BIN_DIR/camilladsp"
  if [ -x "$installed" ]; then
    log "camilladsp present: $installed"
    return
  fi

  local existing
  if existing="$(command -v camilladsp 2>/dev/null)"; then
    run mkdir -p "$(dirname "$installed")"
    run install -m0755 "$existing" "$installed"
    log "Installed CamillaDSP -> $installed"
    return
  fi

  ensure_rust
  # camilladsp is not published to crates.io; build from the official repo,
  # pinned to a release tag for reproducible installs.
  log "Installing camilladsp (pinned to v4.1.3)..."
  run cargo install --git https://github.com/HEnquist/camilladsp --tag v4.1.3

  local built="${CARGO_HOME:-$HOME/.cargo}/bin/camilladsp"
  [ -x "$built" ] || die "camilladsp build completed but binary was not found at $built"
  run mkdir -p "$(dirname "$installed")"
  run install -m0755 "$built" "$installed"
  log "Installed CamillaDSP -> $installed"
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

  local arch asset url tag attempt
  arch="$(detect_arch)"
  asset="oxide-player-${arch}.tar.gz"

  log "Looking up latest release asset: $asset"
  url=""
  for ((attempt = 1; attempt <= RELEASE_ASSET_RETRIES; attempt++)); do
    if url="$(curl -fsSL "$REPO_API/releases/latest" \
          | jq -r --arg a "$asset" '.tag_name as $t | (.assets[]? | select(.name==$a) | {url: .browser_download_url, tag: $t}) | "\(.tag)\t\(.url)"' \
          | head -1)"; then
      :
    else
      url=""
    fi
    if [ -n "$url" ]; then
      break
    fi
    if [ "$attempt" -lt "$RELEASE_ASSET_RETRIES" ]; then
      log "Release asset not available yet (attempt $attempt/$RELEASE_ASSET_RETRIES); retrying in ${RELEASE_ASSET_RETRY_DELAY}s"
      sleep "$RELEASE_ASSET_RETRY_DELAY"
    fi
  done

  if [ -z "$url" ]; then
    warn "No matching release asset found after $RELEASE_ASSET_RETRIES attempts — will build from source."
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
    # The backend serves these as the unprivileged service user; root-owned
    # 0600 copies from scp/umask variants would leave the UI unreadable.
    run chmod -R a+rX "$SHARE_DIR/dist"
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
  run chmod -R a+rX "$SHARE_DIR/dist"
  log "Installed frontend UI -> $SHARE_DIR/dist"
}

# FAT-family filesystems (vfat/exfat/ntfs via fuse) emulate Unix perms with
# mount options. A stock `umask=022` (755/644) leaves the `audio` group
# without write, so `scp` as the admin user (who *is* in `audio` but not the
# owner `oxide`) hits "Permission denied". Patch the fstab entry to be
# group-writable (775/664) when the library mount is FAT-based.
ensure_vfat_writable() {
  local mp="$1"
  [ -n "$mp" ] || return 0
  [ -d "$mp" ] || return 0
  [ -f /etc/fstab ] || return 0
  local fstype=""
  if command -v findmnt >/dev/null 2>&1; then
    fstype="$(findmnt -n -o FSTYPE --target "$mp" 2>/dev/null || true)"
  fi
  if [ -z "$fstype" ]; then
    fstype="$(awk -v mp="$mp" '$2==mp {print $3}' /proc/mounts 2>/dev/null | head -n1)"
  fi
  case "$fstype" in
    vfat|exfat|ntfs|ntfs3|fuseblk) ;;
    *) return 0 ;;
  esac
  # fstab entry must exist — otherwise this is a transient/automounted volume
  if ! awk -v mp="$mp" '$2==mp {found=1; exit} END{exit !found}' /etc/fstab 2>/dev/null; then
    return 0
  fi
  # already group-writable?
  if awk -v mp="$mp" '$2==mp {print $4}' /etc/fstab 2>/dev/null | grep -qE '(^|,)(fmask=0113|dmask=0002|umask=002)(,|$)'; then
    # still ensure remount reflects fstab (systemd may be stale)
    if ! grep -q 'fmask=0113' /proc/mounts 2>/dev/null || ! cat /proc/mounts 2>/dev/null | awk -v mp="$mp" '$2==mp {print $4}' | grep -q 'fmask=0113'; then
      :
    else
      return 0
    fi
  fi
  local uid gid tmp
  uid="$(id -u "$SERVICE_USER" 2>/dev/null || echo 999)"
  gid="$(getent group audio 2>/dev/null | cut -d: -f3)"
  [ -n "$gid" ] || gid=29
  tmp="$(mktemp)"
  awk -v mp="$mp" -v uid="$uid" -v gid="$gid" '
    $2==mp {
      n=split($4, a, ",")
      new=""
      for(i=1;i<=n;i++) {
        if(a[i] ~ /^umask=/ || a[i] ~ /^fmask=/ || a[i] ~ /^dmask=/ || a[i] ~ /^uid=/ || a[i] ~ /^gid=/) continue
        if(a[i]=="") continue
        if(new!="") new=new ","
        new=new a[i]
      }
      if(new!="") new=new ","
      new=new "uid="uid",gid="gid",fmask=0113,dmask=0002"
      $4=new
    }
    {print}
  ' /etc/fstab > "$tmp" && cat "$tmp" > /etc/fstab
  rm -f "$tmp"
  log "Patched /etc/fstab for $mp -> fmask=0113,dmask=0002,uid=$uid,gid=$gid (FAT group-writable)"
  if command -v systemctl >/dev/null 2>&1; then
    systemctl daemon-reload 2>/dev/null || true
    if ! mount -o remount "$mp" 2>/dev/null; then
      local unit
      if command -v systemd-escape >/dev/null 2>&1; then
        unit="$(systemd-escape -p --suffix=mount "$mp" 2>/dev/null || true)"
        [ -n "$unit" ] && systemctl restart "$unit" 2>/dev/null || true
      fi
    fi
  else
    mount -o remount "$mp" 2>/dev/null || true
  fi
}

fix_vfat_music_mounts() {
  # Fix the primary share dir plus any extra library_dirs from a previous
  # install's config (e.g. /mnt/music1 added via Settings -> library_dirs).
  ensure_vfat_writable "$MPD_MUSIC_DIR"
  if [ -f "$CONFIG_DIR/config.json" ] && command -v jq >/dev/null 2>&1; then
    local d
    for d in $(jq -r '.library_dirs[]? // empty' "$CONFIG_DIR/config.json" 2>/dev/null); do
      [ "$d" = "$MPD_MUSIC_DIR" ] && continue
      ensure_vfat_writable "$d"
    done
  elif [ -f "$CONFIG_DIR/config.json" ] && command -v python3 >/dev/null 2>&1; then
    local d
    for d in $(python3 -c 'import json,sys; print("\n".join(json.load(open(sys.argv[1])).get("library_dirs",[])))' "$CONFIG_DIR/config.json" 2>/dev/null); do
      [ "$d" = "$MPD_MUSIC_DIR" ] && continue
      ensure_vfat_writable "$d"
    done
  fi
}

setup_user_dirs() {
  if ! id "$SERVICE_USER" >/dev/null 2>&1; then
    run useradd --system --no-create-home --shell /usr/sbin/nologin "$SERVICE_USER"
  fi
  run usermod -aG audio "$SERVICE_USER" || true
  # SFTP/SCP lands as the admin user (stojce), not oxide. Keep every
  # distinct audio user group-writable-friendly: if the music mount is
  # FAT-family, patch its fstab entry to fmask=0113,dmask=0002 so members
  # of `audio` can write (otherwise scp hits "Permission denied" on 755
  # vfat). No-op on ext4 or when fstab has no entry.
  fix_vfat_music_mounts || true
  # Also ensure the admin can SCP into the tree: put stojce in audio when
  # the mount's group is audio (vfat gid=audio).
  if id stojce >/dev/null 2>&1 && ! id -nG stojce 2>/dev/null | tr ' ' '\n' | grep -qxF audio; then
    if findmnt -n -o FSTYPE --target "$MPD_MUSIC_DIR" 2>/dev/null | grep -qE 'vfat|exfat|ntfs|fuse' \
      || awk -v mp="$MPD_MUSIC_DIR" '$2==mp {print $3}' /proc/mounts 2>/dev/null | grep -qE 'vfat|exfat|ntfs|fuse'; then
      run usermod -aG audio stojce || true
    fi
  fi

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
  run chown -R "$SERVICE_USER:$SERVICE_USER" "$MPD_MUSIC_DIR" 2>/dev/null || true
  # Normalize the music tree so the service user AND MPD (a separate `mpd`
  # system user) can traverse and read it. Non-destructive — only adds
  # read/traverse bits — but repairs trees copied with root ownership and
  # mode 700, which would otherwise scan as an empty library (the scanner
  # reports those as "cannot read library directory" warnings in the log).
  # No-op on vfat/exfat (chmod is emulated via masks).
  run chmod -R u+rwX,go+rX "$MPD_MUSIC_DIR" 2>/dev/null || \
    warn "could not normalize permissions on $MPD_MUSIC_DIR"
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
  local share_path="$MPD_MUSIC_DIR"

  apt_install samba

  if [ -f "$SAMBA_CONFIG" ] && [ ! -f "$SAMBA_CONFIG.pre-oxide" ]; then
    run cp "$SAMBA_CONFIG" "$SAMBA_CONFIG.pre-oxide"
    warn "Backed up existing $SAMBA_CONFIG to $SAMBA_CONFIG.pre-oxide"
  fi

  log "Writing Samba config"
  cat > "$SAMBA_CONFIG" <<SAMBAEOF
[global]
   workgroup = WORKGROUP
   server string = oxide-player
   netbios name = OXIDE-PLAYER
   security = user
   map to guest = Bad User
   guest account = nobody
   include = $SAMBA_SHARES_CONFIG
SAMBAEOF

  run mkdir -p "$share_path" "$(dirname "$SAMBA_SHARES_CONFIG")"
  cat > "$SAMBA_SHARES_CONFIG" <<SHAREEOF
# Managed by oxide-player. The backend updates this file when library sources change.

[Music]
   path = $share_path
   browseable = yes
   read only = no
   guest ok = yes
   force user = $SERVICE_USER
   force group = $SERVICE_USER
   create mask = 0644
   directory mask = 0755
SHAREEOF
  run chown "$SERVICE_USER:$SERVICE_USER" "$SAMBA_SHARES_CONFIG"

  # Ensure the music dir exists with correct ownership
  run chown -R "$SERVICE_USER:$SERVICE_USER" "$share_path"
  run chmod 755 "$share_path"

  # Enable and restart smbd
  run systemctl enable smbd || true
  run systemctl restart smbd || warn "smbd restart failed — check samba config"
  log "Samba share 'Music' → $share_path (guest writable, force-user=$SERVICE_USER)"
}

ensure_samba_shares() {
  [ -f "$SAMBA_CONFIG" ] || return 0
  if grep -Fq 'netbios name = OXIDE-PLAYER' "$SAMBA_CONFIG"; then
    # Migrate the installer-managed legacy [Music] block to the writable
    # fragment used by the library-source API.
    setup_samba
  else
    warn "Samba config is not Oxide-managed; leaving it unchanged (library-source shares need an include)"
  fi
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

setup_bluetooth() {
  log "Enabling Bluetooth stack"

  # Enable and start the BlueZ service so the Bluetooth adapter is usable.
  run systemctl enable bluetooth || true
  run systemctl start bluetooth || warn "bluetooth.service start failed — is there a BT adapter?"

  # bluez-alsa-utils ships the daemon under both names across supported
  # Debian/Ubuntu releases. A2DP sink is what makes this host appear as an
  # audio receiver to an iPhone.
  local bluetoothctl_bin
  bluetoothctl_bin="$(command -v bluetoothctl || echo /usr/bin/bluetoothctl)"
  local daemon
  if command -v bluealsad >/dev/null 2>&1; then
    daemon="$(command -v bluealsad)"
  elif command -v bluealsa >/dev/null 2>&1; then
    daemon="$(command -v bluealsa)"
  else
    warn "BlueALSA daemon not found — Bluetooth phone input will be unavailable."
    return
  fi

  run systemctl disable --now bluealsa.service bluealsad.service bluealsa-aplay.service 2>/dev/null || true
  cat > "$SYSTEMD_DIR/oxide-bluealsa.service" <<EOF
[Unit]
Description=Oxide Player Bluetooth A2DP sink
Documentation=man:bluealsa(8)
Requisite=dbus.service
After=bluetooth.service dbus.service sound.target
Wants=bluetooth.service sound.target

[Service]
Type=dbus
BusName=org.bluealsa
User=root
ExecStart=$daemon -S -p a2dp-source -p a2dp-sink
AmbientCapabilities=CAP_NET_RAW
CapabilityBoundingSet=CAP_NET_RAW
Restart=on-failure
RestartSec=3
[Install]
WantedBy=multi-user.target
EOF
  cat > "$SYSTEMD_DIR/oxide-bluetooth-discoverable.service" <<EOF
[Unit]
Description=Make Oxide Player discoverable for phone audio
After=bluetooth.service
Wants=bluetooth.service

[Service]
Type=oneshot
ExecStart=$bluetoothctl_bin power on
ExecStart=$bluetoothctl_bin pairable on
ExecStart=$bluetoothctl_bin discoverable on
ExecStart=$bluetoothctl_bin discoverable-timeout 0
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
EOF
  log "BlueALSA A2DP sink service configured"
}

setup_asound() {
  # Detect the default audio output device (skip the snd-aloop loopback) and
  # write /etc/asound.conf. All local producers use oxide_loopback so MPD,
  # Bluetooth A2DP, and AirPlay can share the CamillaDSP input safely.
  local conf="$ASOUND_CONFIG"

  if ! command -v aplay >/dev/null 2>&1; then
    warn "aplay not found — skipping ALSA default config."
    warn "Install alsa-utils and run 'aplay -l' to find your device."
    return
  fi

  local card="" dev="" name=""
  while IFS= read -r line; do
    if [[ $line =~ ^card[[:space:]]+([0-9]+):[[:space:]]([A-Za-z].+),[[:space:]]+device[[:space:]]+([0-9]+):[[:space:]](.+) ]]; then
      local c="${BASH_REMATCH[1]}"
      local cname="${BASH_REMATCH[2]}"
      local d="${BASH_REMATCH[3]}"
      local dname="${BASH_REMATCH[4]}"
      if [[ $cname != *Loopback* ]] && [[ $cname != *loopback* ]]; then
        card="$c"
        dev="$d"
        name="$cname — $dname"
        break
      fi
    fi
  done < <(aplay -l 2>/dev/null || true)

  log "Writing ALSA configuration ($conf)"
  cat > "$conf" <<'ASOUNDEOF'
# oxide-player: shared input path for MPD, Bluetooth A2DP, and AirPlay
# Producers write to oxide_loopback; CamillaDSP captures hw:Loopback,1.
pcm.oxide_loopback {
    type plug
    slave.pcm {
        type dmix
        ipc_key 0x4f584944
        slave {
            pcm "hw:Loopback,0,0"
            format S16_LE
            rate 48000
            channels 2
            period_size 1024
            buffer_size 4096
        }
    }
}

ctl.oxide_loopback {
    type hw
    card Loopback
}
ASOUNDEOF

  if [ -z "$card" ]; then
    warn "No audio hardware detected (only loopback found)."
    warn "CamillaDSP will use 'hw:DAC'. Edit /etc/asound.conf manually."
    return
  fi

  log "Detected audio output: card $card ($name)"
  cat >> "$conf" <<ASOUNDEOF

# oxide-player: default audio output → card $card, device $dev
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

ensure_asound_loopback() {
  local conf="$ASOUND_CONFIG"
  if [ -f "$conf" ] && grep -Fq 'pcm.oxide_loopback' "$conf"; then
    log "ALSA shared loopback already configured: $conf"
    return 1
  fi

  cat >> "$conf" <<'ASOUNDEOF'

# oxide-player: shared input path for MPD, Bluetooth A2DP, and AirPlay
pcm.oxide_loopback {
    type plug
    slave.pcm {
        type dmix
        ipc_key 0x4f584944
        slave {
            pcm "hw:Loopback,0,0"
            format S16_LE
            rate 48000
            channels 2
            period_size 1024
            buffer_size 4096
        }
    }
}

ctl.oxide_loopback {
    type hw
    card Loopback
}
ASOUNDEOF
  log "Added shared ALSA loopback to $conf"
  return 0
}

setup_airplay() {
  log "Configuring AirPlay receiver"
  run mkdir -p "$(dirname "$AIRPLAY_CONFIG")"
  cat > "$AIRPLAY_CONFIG" <<EOF
general =
{
    name = "$AIRPLAY_NAME";
    output_backend = "alsa";
    mdns_backend = "avahi";
};

alsa =
{
    output_device = "oxide_loopback";
};
EOF
  run chown "$SERVICE_USER:$SERVICE_USER" "$AIRPLAY_CONFIG" 2>/dev/null || true
}

write_mpd_config() {
  local conf="$MPD_CONFIG"
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
include             "$DATA_DIR/mpd-outputs.d/*.conf"

audio_output {
    type          "alsa"
    name          "camilladsp-loopback"
    device        "oxide_loopback"
    format        "48000:16:2"
    mixer_type    "software"
    dop           "no"
}
EOF
  run mkdir -p "$DATA_DIR/playlists"
  run chown -R "$SERVICE_USER:$SERVICE_USER" "$DATA_DIR" 2>/dev/null || true
}

#
# Visualizer fifo output: MPD writes a copy of the played stream to a fifo that
# the backend's FFT visualizer reads. Unlike ALSA loopback capture this works
# in every output mode (Bluetooth / DSP loopback / analog) and avoids
# snd-aloop substream contention with CamillaDSP's capture.
#
write_visualizer_fifo() {
  local fifo_conf="$DATA_DIR/mpd-outputs.d/visualizer-fifo.conf"
  log "Writing visualizer fifo output ($fifo_conf)"
  run mkdir -p "$DATA_DIR/mpd-outputs.d"
  cat > "$fifo_conf" <<EOF
audio_output {
    type        "fifo"
    name        "visualizer"
    path        "$DATA_DIR/visualizer.fifo"
    format      "44100:16:2"
}
EOF
  run chown "$SERVICE_USER:$SERVICE_USER" "$fifo_conf" 2>/dev/null || true
}


#
# Repair an existing MPD config during --update. Older installations may not
# have the managed output include because --update previously skipped all MPD
# setup. Keep the file owner and mode unchanged by updating it in place.
ensure_mpd_include() {
  local conf="$MPD_CONFIG"
  local managed_include="include \"$DATA_DIR/mpd-outputs.d/*.conf\""

  if [ ! -f "$conf" ]; then
    warn "MPD config $conf does not exist — skipping managed output include repair."
    return 1
  fi
  if [ ! -s "$conf" ]; then
    warn "MPD config $conf is empty — restoring the managed local-library configuration."
    write_mpd_config
    return 0
  fi
  if grep -Fqx "$managed_include" "$conf"; then
    log "MPD config already includes managed outputs: $conf"
    return 1
  fi

  local tmp="${conf}.oxide-player.tmp"
  awk -v managed_include="$managed_include" '
    BEGIN { done = 0 }
    {
      if ($0 ~ /^[[:space:]]*include[[:space:]]+"/ &&
          $0 ~ /mpd-outputs[.]d/ &&
          $0 ~ /[*][.]conf/) {
        if (!done) {
          print managed_include
          done = 1
        }
        next
      }
      if (!done && $0 ~ /^[[:space:]]*audio_output[[:space:]]*[{]/) {
        print managed_include
        done = 1
      }
      print
    }
    END {
      if (!done) print managed_include
    }
  ' "$conf" > "$tmp"
  if [ ! -s "$tmp" ] || ! grep -Fq "$managed_include" "$tmp"; then
    rm -f "$tmp"
    die "refusing to replace $conf: generated MPD config is empty or missing the managed output include"
  fi
  if ! cat "$tmp" > "$conf"; then
    rm -f "$tmp"
    die "cannot update MPD config $conf"
  fi
  rm -f "$tmp"
  if [ ! -s "$conf" ] || ! grep -Fq "$managed_include" "$conf"; then
    die "MPD config $conf failed post-write validation"
  fi
  log "Added managed output include to $conf"
}

# Older installs have a managed CamillaDSP loopback output without an
# explicit mixer. MPD then probes the ALSA PCM mixer ("PCM"), which snd-aloop
# does not provide, so setvol fails while DSP is active. Repair only Oxide's
# loopback block and leave user-managed outputs untouched.
ensure_mpd_loopback_mixer() {
  local conf="$MPD_CONFIG"
  if [ ! -f "$conf" ] || [ ! -s "$conf" ]; then
    return 1
  fi
  if ! grep -Eq '^[[:space:]]*name[[:space:]]+"camilladsp-loopback"[[:space:]]*$' "$conf"; then
    return 1
  fi

  local tmp
  tmp="$(mktemp "${conf}.oxide-player.tmp.XXXXXX")" || die "cannot create MPD config temp file"
  if ! awk '
    function clear_block( i) {
      for (i = 1; i <= block_len; i++) delete block[i]
      block_len = 0
    }
    function flush_block( i, line, loopback, mixer_seen, has_name, has_device) {
      if (!in_output) return

      has_name = 0
      has_device = 0
      for (i = 1; i <= block_len; i++) {
        if (block[i] ~ /^[[:space:]]*name[[:space:]]+"camilladsp-loopback"[[:space:]]*$/) {
          has_name = 1
        }
        if (block[i] ~ /^[[:space:]]*device[[:space:]]+"(oxide_loopback|hw:Loopback,0)"[[:space:]]*$/) {
          has_device = 1
        }
      }
      loopback = has_name && has_device
      if (!loopback) {
        for (i = 1; i <= block_len; i++) print block[i]
        clear_block()
        in_output = 0
        return
      }

      mixer_seen = 0
      for (i = 1; i <= block_len; i++) {
        line = block[i]
        if (line ~ /^[[:space:]]*mixer_type[[:space:]]+"/) {
          mixer_seen = 1
          if (line ~ /^[[:space:]]*mixer_type[[:space:]]+"software"[[:space:]]*$/) {
            print line
          } else {
            print "    mixer_type    \"software\""
          }
        } else {
          if (line ~ /^[[:space:]]*}[[:space:]]*$/ && !mixer_seen) {
            print "    mixer_type    \"software\""
          }
          print line
        }
      }
      clear_block()
      in_output = 0
    }
    /^[[:space:]]*audio_output[[:space:]]*[{]/ {
      if (in_output) flush_block()
      in_output = 1
      block_len = 1
      block[block_len] = $0
      next
    }
    in_output {
      block[++block_len] = $0
      if ($0 ~ /^[[:space:]]*}[[:space:]]*$/) flush_block()
      next
    }
    { print }
    END {
      if (in_output) flush_block()
    }
  ' "$conf" > "$tmp"; then
    rm -f "$tmp"
    die "cannot parse MPD config $conf"
  fi

  if cmp -s "$tmp" "$conf"; then
    rm -f "$tmp"
    return 1
  fi
  if ! cat "$tmp" > "$conf"; then
    rm -f "$tmp"
    die "cannot update CamillaDSP loopback mixer in $conf"
  fi
  rm -f "$tmp"
  log "Repaired CamillaDSP loopback mixer in $conf"
  return 0
}


write_camilladsp_config() {
  log "Writing CamillaDSP config ($CAMILLADSP_CONFIG)"
  cat > "$CAMILLADSP_CONFIG" <<'EOF'
devices:
  capture:
    type: Alsa
    channels: 2
    device: hw:Loopback,1
    format: S16_LE
  playback:
    type: Alsa
    channels: 2
    device: default
    format: S16_LE
  samplerate: 48000
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
  local generated
  generated="$(cat <<EOF
{
  "mpd_host": "127.0.0.1",
  "mpd_port": 6600,
  "listen": "$LISTEN",
  "data_dir": "$DATA_DIR",
  "mpd_config": "$MPD_CONFIG",
  "bluetooth_reconnect_on_startup": true,
  "library_dirs": $dirs_json,
  "static_dir": "$SHARE_DIR/dist",
  "camilladsp_config_path": "$CAMILLADSP_CONFIG",
  "camilladsp_ws_url": "$CAMILLADSP_WS",
  "camilladsp_binary": "$BIN_DIR/camilladsp",
  "visualizer_fft": true,
  "visualizer_capture_device": "hw:Loopback,1",
  "visualizer_capture_rate": 44100,
  "visualizer_fifo": "$DATA_DIR/visualizer.fifo"
}
EOF
)"
  # Re-running the installer (reinstall) must not wipe settings the user
  # edited in the web UI (Music library sources, listen, ...). When a config
  # already exists, keep every key it defines and only fill in keys the
  # installer manages that are missing (e.g. defaults added by newer
  # versions). Fresh installs get the full generated config.
  if [[ -f "$CONFIG_DIR/config.json" ]]; then
    if command -v jq >/dev/null 2>&1; then
      # jq `left * right` keeps the right operand's values on collisions, so
      # put the existing config on the right: existing keys win, generated keys
      # fill in what the old config lacks. Remove settings retired by the
      # current config schema during the same migration.
      local merged
      if merged="$(jq --slurpfile g <(printf '%s\n' "$generated") '$g[0] * . | del(.mpd_music_directory)' "$CONFIG_DIR/config.json")"; then
        log "Preserving existing config; filling in only missing keys"
        printf '%s\n' "$merged" > "$CONFIG_DIR/config.json"
        run chown "$SERVICE_USER:$SERVICE_USER" "$CONFIG_DIR/config.json"
        return
      fi
      warn "Existing config is not valid JSON; regenerating from defaults"
    else
      warn "jq not found; leaving existing config unchanged"
      return
    fi
  fi
  printf '%s\n' "$generated" > "$CONFIG_DIR/config.json"
  run chown "$SERVICE_USER:$SERVICE_USER" "$CONFIG_DIR/config.json"
}

# Grant the service user passwordless `systemctl reboot` and `systemctl poweroff`
# so the backend's `POST /api/system/shutdown|restart` can power off/reboot the
# machine as an unprivileged user.
write_power_sudoers() {
  local rule="$SUDOERS_RULE"
  log "Writing sudoers power rule ($rule)"
  cat > "$rule" <<EOF
$SERVICE_USER ALL=(root) NOPASSWD: /usr/bin/systemctl reboot, /usr/bin/systemctl poweroff
EOF
  run chmod 440 "$rule"
  run visudo -cf "$rule"
}

install_units() {
  log "Installing systemd units"
  write_power_sudoers
  local _cam_host="${CAMILLADSP_WS#ws://}"
  _cam_host="${_cam_host%:*}"
  local _cam_port="${CAMILLADSP_WS##*:}"
  local _shairport_bin
  _shairport_bin="$(command -v shairport-sync || echo /usr/bin/shairport-sync)"
  cat > /etc/systemd/system/camilladsp.service <<EOF
[Unit]
Description=CamillaDSP audio processor
After=network.target sound.target
Wants=sound.target

[Service]
Type=simple
User=$SERVICE_USER
ExecStart=$BIN_DIR/camilladsp $CAMILLADSP_CONFIG -a $_cam_host -p $_cam_port
Restart=on-failure

[Install]
WantedBy=multi-user.target
EOF

  cat > /etc/systemd/system/oxide-airplay.service <<EOF
[Unit]
Description=Oxide Player AirPlay receiver
After=network.target avahi-daemon.service sound.target
Wants=network.target avahi-daemon.service sound.target

[Service]
Type=simple
User=$SERVICE_USER
SupplementaryGroups=audio
ExecStart=$_shairport_bin -c $AIRPLAY_CONFIG
Restart=on-failure
RestartSec=3

[Install]
WantedBy=multi-user.target
EOF

  if [ -f "$SRC_DIR/contrib/systemd/oxide-player.service" ]; then
    run install -Dm0644 "$SRC_DIR/contrib/systemd/oxide-player.service" /etc/systemd/system/oxide-player.service
  else
    cat > /etc/systemd/system/oxide-player.service <<EOF
[Unit]
Description=oxide-player — audiophile MPD + CamillaDSP controller
After=network.target mpd.service camilladsp.service
Wants=mpd.service camilladsp.service

[Service]
Type=simple
User=$SERVICE_USER
WorkingDirectory=$DATA_DIR
ExecStart=$BIN_DIR/oxide-player -c $CONFIG_DIR/config.json
Nice=-5
Restart=on-failure
RestartSec=3
# Allow binding to port 80 (privileged port) as non-root user.
# Do NOT add CapabilityBoundingSet here: it drops CAP_SETUID/CAP_SETGID and
# breaks the backend's `sudo -n systemctl reboot|poweroff` power buttons.
AmbientCapabilities=CAP_NET_BIND_SERVICE


[Install]
WantedBy=multi-user.target
EOF
  fi

  run mkdir -p /etc/modules-load.d
  echo "snd-aloop" > /etc/modules-load.d/oxide.conf
  run systemctl disable --now shairport-sync.service || true
  run systemctl daemon-reload
  run systemctl enable mpd camilladsp oxide-player oxide-airplay
  run systemctl enable oxide-bluealsa oxide-bluetooth-discoverable || warn "Bluetooth input service could not be enabled"
  run systemctl restart mpd camilladsp oxide-airplay oxide-player
  run systemctl restart oxide-bluealsa || warn "Bluetooth A2DP service could not be started"
  run systemctl restart oxide-bluetooth-discoverable || warn "Bluetooth discoverability could not be enabled"
}

write_motd() {
  # Login welcome banner (Ubuntu/Debian pam_motd). The script under
  # /etc/update-motd.d/ runs on every SSH login, so the version and service
  # status shown are always live. Quoted heredoc keeps the inner script's
  # $vars and \033 escapes intact for login-time evaluation.
  log "Installing login MOTD"
  local _port="${LISTEN##*:}"
  local _port_suffix=""
  [ "$_port" != "80" ] && _port_suffix=":$_port"
  local _motd
  _motd="$(cat <<'MOTD_EOF'
#!/bin/sh
# Oxide Player status MOTD (managed by install.sh)

IP=$(hostname -I 2>/dev/null | awk '{print $1}')
HOST=$(hostname -s 2>/dev/null | tr '[:upper:]' '[:lower:]')

cat <<'ART'

   █████   ██   ██  ███████  █████    ███████
  ██   ██   ██ ██      █     ██   ██  ██
  ██   ██    ███       █     ██   ██  ██
  ██   ██     █        █     ██   ██  █████
  ██   ██    ███       █     ██   ██  ██
  ██   ██   ██ ██      █     ██   ██  ██
   █████   ██   ██  ███████  █████    ███████

  ── Audiophile Music Server ──
ART

VER=$(curl -s --max-time 2 "http://localhost__PORT_SUFFIX__/api/version" 2>/dev/null | sed -n 's/.*"version":"\([^"]*\)".*/\1/p')
[ -n "$VER" ] && printf '  Version : %s\n' "$VER"

if systemctl is-active --quiet oxide-player; then
    printf '  Service : \033[0;32mactive\033[0m\n'
else
    printf '  Service : \033[0;31mINACTIVE\033[0m\n'
fi

printf '  Web UI  : http://%s__PORT_SUFFIX__/\n' "$IP"
printf '  mDNS    : http://%s.local__PORT_SUFFIX__/\n' "$HOST"
printf '  Music   : smb://%s/Music\n' "$IP"
printf '\n'
MOTD_EOF
)"
  _motd="${_motd//__PORT_SUFFIX__/$_port_suffix}"
  run mkdir -p /etc/update-motd.d
  printf '%s\n' "$_motd" > /etc/update-motd.d/99-oxide-player
  run chmod 755 /etc/update-motd.d/99-oxide-player
}

# Wall display kiosk: a single-app Wayland session (cage) running Chromium
# pointed at the backend's /kiosk view on the attached HDMI touchscreen.
# Blank/wake policy: playing or paused keeps the panel lit; continuous stopped
# playback past KIOSK_IDLE_SECONDS blanks it; touch wakes it (swayidle resume).
write_kiosk() {
  log "Installing wall display kiosk (cage + Chromium)"
  # Fault isolation: a host where a kiosk package is unavailable (e.g. the
  # snap-transitional chromium-browser without snapd) must not abort the whole
  # Wall display is opt-in: Chromium only launches on the HDMI panel when
  # KIOSK_ENABLED=1. Default off keeps headless installs on the login TTY.
  if [ "${KIOSK_ENABLED:-0}" != "1" ]; then
    log "Wall display kiosk disabled (set KIOSK_ENABLED=1 to provision it) — playback is unaffected"
    return 0
  fi
  # install — playback and every other feature stay unaffected.
  if ! apt_install cage swayidle wlopm chromium-browser; then
    warn "Kiosk packages unavailable — skipping wall display setup (playback is unaffected)"
    return 0
  fi
  # The transitional chromium-browser package contains no files — the real
  # browser is the snap. Resolve an actual executable and fail soft when none
  # can be provided.
  _browser=""
  for _cand in /usr/bin/chromium-browser /snap/bin/chromium /usr/bin/chromium; do
    [ -x "$_cand" ] && _browser="$_cand" && break
  done
  if [ -z "$_browser" ]; then
    log "No chromium binary found — installing the chromium snap"
    # On a freshly reinstalled system snapd may still be initializing; wait
    # for its socket before attempting the install, then retry once.
    for _i in 1 2 3 4 5 6; do
      [ -S /run/snapd.socket ] && systemctl is-active --quiet snapd && break
      log "Waiting for snapd to become ready ($_i/6)"
      sleep 5
    done
    if run snap install chromium; then
      _browser="/snap/bin/chromium"
    else
      # Snap downloads can stall on slow links; one clean retry.
      warn "Chromium snap install failed — retrying once"
      run snap install chromium && _browser="/snap/bin/chromium"
    fi
    [ -n "$_browser" ] || _browser="$(command -v chromium 2>/dev/null || true)"
  fi
  if [ -z "$_browser" ]; then
    warn "Chromium unavailable — skipping wall display setup (playback is unaffected)"
    return 0
  fi

  # The snap-packaged browser needs a writable home for its data directory,
  # and logind tears down /run/user/<uid> between sessions without linger -
  # which deletes the Wayland socket underneath the compositor. System
  # accounts are created without either, so provide both here.
  run mkdir -p "/home/$SERVICE_USER"
  run chown "$SERVICE_USER:$SERVICE_USER" "/home/$SERVICE_USER"
  run loginctl enable-linger "$SERVICE_USER" || warn "Could not enable linger for $SERVICE_USER"
  # Newer systemd registers PAM sessions as lightweight by default; light
  # sessions never become active on the seat and wlroots waits forever.
  cat > /etc/pam.d/oxide-kiosk <<EOF
auth     required   pam_permit.so
account  required   pam_permit.so
session  required   pam_unix.so
session  optional   pam_systemd.so class=user
EOF
  local _port="${LISTEN##*:}"
  local _port_suffix=""
  [ "$_port" != "80" ] && _port_suffix=":$_port"
  local _idle="${KIOSK_IDLE_SECONDS:-600}"

  # Playback-aware blanker. Decides *when* to blank; wake-on-touch belongs to
  # swayidle's resume hook, which also stamps a wake file so this watcher
  # restarts its idle clock after a manual wake instead of re-blanking at once.
  local _watcher
  _watcher="$(cat <<'WATCHER_EOF'
#!/bin/sh
# oxide-kiosk-idle-watcher — blank the wall display after continuous stopped playback.
BASE_URL="http://127.0.0.1__PORT_SUFFIX__"
IDLE_SECONDS="__KIOSK_IDLE_SECONDS__"
POLL=15
stopped=0
blanked=0
wlopm_fails=0
wake_seen=0
[ -f "$WAKE_FILE" ] && wake_seen=$(stat -c %Y "$WAKE_FILE" 2>/dev/null || echo 0)
while :; do
    state="$(curl -s --max-time 5 "$BASE_URL/api/status" 2>/dev/null \
        | sed -n 's/.*"state":"\([a-z]*\)".*/\1/p')"
    if [ -z "$state" ]; then
        # Backend unreachable: hold current power state, pause the idle clock.
        sleep "$POLL"
        continue
    fi
    if [ "$state" = "stopped" ]; then
        if [ -f "$WAKE_FILE" ]; then
            wake_now=$(stat -c %Y "$WAKE_FILE" 2>/dev/null || echo 0)
            if [ "$wake_now" -gt "$wake_seen" ]; then
                wake_seen=$wake_now
                stopped=0
                blanked=0
            fi
        fi
        if [ "$blanked" -eq 0 ]; then
            stopped=$((stopped + POLL))
            if [ "$stopped" -ge "$IDLE_SECONDS" ]; then
                # Cap consecutive failures so a wedged compositor cannot turn
                # this loop into unbounded identical retries; retrying resumes
                # as soon as playback leaves stopped.
                if wlopm --off '*'; then
                    blanked=1
                    wlopm_fails=0
                else
                    wlopm_fails=$((wlopm_fails + 1))
                    [ "$wlopm_fails" -ge 5 ] && blanked=1
                fi
            fi
        fi
    else
        # Playing or paused counts as activity (paused keeps the panel lit).
        stopped=0
        wlopm_fails=0
        if [ "$blanked" -eq 1 ]; then
            wlopm --on '*' || true
            blanked=0
        fi
    fi
    sleep "$POLL"
done
WATCHER_EOF
)"
  _watcher="${_watcher//__PORT_SUFFIX__/$_port_suffix}"
  _watcher="${_watcher//__KIOSK_IDLE_SECONDS__/$_idle}"
  printf '%s\n' "$_watcher" > "$BIN_DIR/oxide-kiosk-idle-watcher"
  run chmod 755 "$BIN_DIR/oxide-kiosk-idle-watcher"

  # Session launcher. Order matters:
  #   1. Wait for the backend's HTTP endpoint before starting Chromium —
  #      After= only orders exec start, not port bind; without this wait a slow
  #      boot leaves a browser error page on the panel forever (nothing retries).
  #   2. Start cage, then poll for its Wayland socket — but bail out if cage
  #      dies meanwhile (headless box) so systemd's restart policy engages
  #      instead of stalling ~20s per attempt.
  #   3. Helpers run only once WAYLAND_DISPLAY exists, and swayidle runs under
  #      a tiny supervision loop so a crash cannot silently kill wake-on-touch.
  #   swayidle's timeout is deliberately shorter than the watcher poll: resume
  #      fires when input follows an idle stretch, so frequent-enough touches
  #      keep stamping activity and the watcher never blanks under an active
  #      finger.
  local _session
  _session="$(cat <<'SESSION_EOF'
#!/bin/sh
# oxide-kiosk-session — single-app Wayland kiosk session for the wall display.
export XDG_SESSION_TYPE=wayland
# PAM does not always export this into the unit environment; snap browsers
# need it to find the compositor socket.
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
# Wait for the backend before launching the browser (After= orders exec, not bind).
i=0
until curl -sf --max-time 2 "http://127.0.0.1__PORT_SUFFIX__/api/version" >/dev/null 2>&1; do
    [ "$i" -ge 60 ] && break
    sleep 1
    i=$((i + 1))
done

cage -d -s -- __BROWSER__ \
    --ozone-platform=wayland --enable-features=UseOzonePlatform \
    --kiosk --noerrdialogs --disable-infobars \
    --disable-session-crashed-bubble --hide-crash-restore-bubble \
    "http://127.0.0.1__PORT_SUFFIX__/kiosk?panel=1&idle=__KIOSK_IDLE_SECONDS__" &
CAGE_PID=$!

i=0
while :; do
    [ -n "$(ls "$XDG_RUNTIME_DIR"/wayland-* 2>/dev/null)" ] && break
    # Cage exited (no DRM seat / headless): fail now so Restart=always and the
    # start limit engage instead of burning the poll budget as a zombie.
    kill -0 "$CAGE_PID" 2>/dev/null || exit 1
    [ "$i" -ge 100 ] && break
    sleep 0.2
    i=$((i + 1))
done
_wl="$(ls "$XDG_RUNTIME_DIR"/wayland-* 2>/dev/null | head -n 1)"
[ -n "$_wl" ] && WAYLAND_DISPLAY="$(basename "$_wl")" && export WAYLAND_DISPLAY

# Supervised swayidle: if it ever exits, relaunch it — its resume hook is the
# only wake-on-touch path, so losing it silently would strand a blanked panel.
( while :; do
      swayidle -w timeout 10 'true' \
          resume 'wlopm --on "*"; touch /tmp/oxide-kiosk-wake'
      sleep 2
  done ) &
"__BINDIR__/oxide-kiosk-idle-watcher" &

wait "$CAGE_PID"
SESSION_EOF
)"
  _session="${_session//__PORT_SUFFIX__/$_port_suffix}"
  _session="${_session//__KIOSK_IDLE_SECONDS__/$_idle}"
  _session="${_session//__BINDIR__/$BIN_DIR}"
  _session="${_session//__BROWSER__/$_browser}"
  printf '%s\n' "$_session" > "$BIN_DIR/oxide-kiosk-session"
  run chmod 755 "$BIN_DIR/oxide-kiosk-session"

  # Switch to the kiosk VT before the compositor starts: logind activates the
  # session but does not change the foreground VT on newer releases, and
  # wlroots waits for exactly that. The leading '-' tolerates headless boots.
  if [ -f "$SRC_DIR/contrib/systemd/oxide-kiosk.service" ]; then
    run install -Dm0644 "$SRC_DIR/contrib/systemd/oxide-kiosk.service" "$SYSTEMD_DIR/oxide-kiosk.service"
    sed -i "\|^ExecStart=|i ExecStartPre=-/usr/bin/chvt ${KIOSK_TTY:-7}" "$SYSTEMD_DIR/oxide-kiosk.service"
  else
    cat > "$SYSTEMD_DIR/oxide-kiosk.service" <<EOF
# Oxide Player wall display kiosk: a single-app Wayland session (cage) that
# runs Chromium pointed at the backend's /kiosk view on the HDMI panel.
# Keep this copy in sync with contrib/systemd/oxide-kiosk.service.
[Unit]
Description=Oxide Player wall display kiosk (cage + Chromium)
Documentation=https://github.com/OxideAI/oxide-player
After=systemd-logind.service dbus.socket oxide-player.service
Wants=oxide-player.service
# Headless boot (no panel attached): fail fast instead of looping forever.
StartLimitIntervalSec=60
StartLimitBurst=5

[Service]
Type=simple
User=$SERVICE_USER
Group=$SERVICE_USER
PAMName=oxide-kiosk
TTYPath=/dev/tty7
StandardInput=tty
StandardOutput=journal
StandardError=journal
UtmpIdentifier=tty7
ExecStartPre=-/usr/bin/chvt ${KIOSK_TTY:-7}
ExecStart=$BIN_DIR/oxide-kiosk-session
# Restart on any exit so a crashed browser lands back on the wall view.
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
EOF
  fi

  run systemctl daemon-reload
  run systemctl enable oxide-kiosk || warn "Could not enable oxide-kiosk.service"
  run systemctl restart oxide-kiosk || warn "oxide-kiosk not started (headless or no seat?) — playback is unaffected"
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
  log "AirPlay:       select '$AIRPLAY_NAME' on an iPhone/iPad on the same LAN"
  log "Bluetooth:     pair an iPhone with 'Oxide Player', then enable A2DP input in Settings"
  log "Edit config:   $CONFIG_DIR/config.json"
  log "Logs:          journalctl -u oxide-player -f"
  log ""
  log "Copy your music files into the shared Music folder, then scan in the web UI:"
  log "  Settings → Music library sources → Rescan library"
  log ""
  log "If the library ever looks empty despite music being present, check:"
  log "  journalctl -u oxide-player | grep 'cannot read library directory'"
  log "That means files were copied with root ownership. Fix with:"
  log "  sudo bash install.sh --fix-perms   (then rescan)"
}

# Repair the music library tree so the service user and MPD can read it. This
# is the exact fix for a library that silently scans as empty because albums
# were copied as root (mode 700). Safe to run at any time — chown + a
# non-destructive chmod that only adds read/traverse bits.
fix_music_perms() {
  need_root
  check_linux
  [ -d "$MPD_MUSIC_DIR" ] || die "Music dir $MPD_MUSIC_DIR does not exist — nothing to fix."
  fix_vfat_music_mounts || true
  log "Repairing music library permissions at $MPD_MUSIC_DIR"
  run chown -R "$SERVICE_USER:$SERVICE_USER" "$MPD_MUSIC_DIR" 2>/dev/null || true
  run chmod -R u+rwX,go+rX "$MPD_MUSIC_DIR" 2>/dev/null || true
  log "Done. Rescan the library in the web UI (Settings → Music library sources → Rescan)"
}

check_linux() {
  case "$(uname -s)" in
    Linux) ;;
    *) die "oxide-player only runs on Linux (you're on $(uname -s))." ;;
  esac
}

remove_path() {
  local path="$1"
  case "$path" in
    ""|/|.) die "refusing to remove unsafe path '$path'" ;;
  esac
  [ -e "$path" ] || return 0
  run rm -rf -- "$path"
}

restore_backup_or_warn() {
  local path="$1"
  local backup="${path}.pre-oxide"
  if [ -f "$backup" ]; then
    run mv "$backup" "$path"
  else
    warn "No Oxide backup for $path; leaving it unchanged."
  fi
}

do_uninstall() {
  need_root
  check_linux

  local units=(
    oxide-player.service
    oxide-airplay.service
    oxide-bluealsa.service
    oxide-bluetooth-discoverable.service
    oxide-kiosk.service
    camilladsp.service
  )
  run systemctl disable --now "${units[@]}" 2>/dev/null || true
  run systemctl daemon-reload || true

  for unit in "${units[@]}"; do
    remove_path "$SYSTEMD_DIR/$unit"
  done

  remove_path "$BIN_DIR/oxide-player"
  remove_path "$BIN_DIR/camilladsp"
  remove_path "$BIN_DIR/oxide-kiosk-idle-watcher"
  remove_path "$BIN_DIR/oxide-kiosk-session"
  remove_path /etc/pam.d/oxide-kiosk
  # Linger keeps /run/user/<uid> alive for the kiosk session; without the
  # kiosk it only wastes a runtime dir. Home dir and browser profile are
  # preserved like the rest of the user's data.
  loginctl disable-linger "$SERVICE_USER" 2>/dev/null || true
  remove_path "$SHARE_DIR"
  remove_path "/etc/avahi/services/oxide-player.service"
  remove_path "$SUDOERS_RULE"
  remove_path /etc/update-motd.d/99-oxide-player


  if [ -f "$ASOUND_CONFIG" ] && grep -Fq '# oxide-player:' "$ASOUND_CONFIG"; then
    remove_path "$ASOUND_CONFIG"
  else
    warn "Leaving $ASOUND_CONFIG unchanged; it is not marked as Oxide-managed."
  fi

  if [ -f "$AIRPLAY_CONFIG" ] &&
    grep -Fq 'output_device = "oxide_loopback";' "$AIRPLAY_CONFIG"; then
    remove_path "$AIRPLAY_CONFIG"
  fi

  if [ -f "$MPD_CONFIG.pre-oxide" ]; then
    restore_backup_or_warn "$MPD_CONFIG"
    run systemctl restart mpd || true
  else
    warn "No Oxide MPD backup found; leaving $MPD_CONFIG unchanged."
  fi

  if [ -f "$SAMBA_CONFIG.pre-oxide" ]; then
    run mv "$SAMBA_CONFIG.pre-oxide" "$SAMBA_CONFIG"
    run systemctl restart smbd || true
  else
    warn "No Oxide Samba backup found; leaving $SAMBA_CONFIG unchanged."
  fi

  if [ -f /etc/modules-load.d/oxide.conf ] &&
    [ "$(tr -d '\n' < /etc/modules-load.d/oxide.conf)" = "snd-aloop" ]; then
    remove_path /etc/modules-load.d/oxide.conf
  fi
  run systemctl restart avahi-daemon || true

  log "Uninstall complete."
  log "Preserved config: $CONFIG_DIR"
  log "Preserved data and music: $DATA_DIR and $MPD_MUSIC_DIR"
  log "The $SERVICE_USER account and installed packages were not removed."
  log "Remove preserved files manually only after confirming you no longer need them."
}

do_update() {
  need_root
  apt_install curl git build-essential pkg-config libssl-dev libasound2-dev \
              bluez-alsa-utils shairport-sync
  ensure_camilladsp
  fetch_source
  build_backend
  build_frontend
  fix_vfat_music_mounts || true
  ensure_mpd_include || true
  ensure_mpd_loopback_mixer || true
  write_camilladsp_config
  ensure_samba_shares || true
  write_visualizer_fifo || true
  write_oxide_config
  setup_bluetooth
  setup_airplay
  install_units
  write_motd
  write_kiosk

  log "Update complete."
  finish
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
  if $uninstall_mode; then
    do_uninstall
    return
  fi
  if $update_mode; then
    do_update
    return
  fi
  if $fix_perms_mode; then
    fix_music_perms
    return
  fi
  need_root
  check_linux
  detect_os
  apt_install curl jq git build-essential pkg-config libssl-dev \
              libasound2-dev alsa-utils mpd mpc ffmpeg samba avahi-daemon bluez \
              bluez-alsa-utils shairport-sync
  check_dependencies
  ensure_camilladsp
  fetch_source
  build_backend
  build_frontend
  setup_user_dirs
  setup_asound
  setup_samba
  setup_avahi
  setup_bluetooth
  write_mpd_config
  write_camilladsp_config
  write_visualizer_fifo
  write_oxide_config
  setup_airplay
  install_units
  write_motd
  write_kiosk
  finish
}

if [ "${OXIDE_INSTALLER_TEST:-0}" != "1" ]; then
  main
fi
