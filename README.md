# Oxide

A self-hosted music server for a local library. The Rust backend talks to MPD for decoding and transport, uses CamillaDSP for resampling and parametric EQ, and serves a React web UI. It also has an optional full-screen kiosk view with a real-time FFT visualizer. The intended setup is a headless Linux box connected to a USB DAC and reachable over your LAN.

Oxide scans the library into SQLite and serves optimized cover art. Its JSON API controls playback, queues, DSP, radio, Bluetooth, output devices, and playlists. The backend stays in front of MPD as a control and metadata layer.

## Screenshots

| Library (desktop)                        | Album + now playing                          | Queue panel                          |
| ---------------------------------------- | -------------------------------------------- | ------------------------------------ |
| ![Library](docs/screenshots/library.png) | ![Album](docs/screenshots/album-playing.png) | ![Queue](docs/screenshots/queue.png) |

| Settings (DSP / devices)                   | Kiosk mode                           | Library (mobile)                               |
| ------------------------------------------ | ------------------------------------ | ---------------------------------------------- |
| ![Settings](docs/screenshots/settings.png) | ![Kiosk](docs/screenshots/kiosk.png) | ![Mobile](docs/screenshots/mobile-library.png) |

## Features

- Browse albums, artists, and tracks with full-text search, CUE-sheet track addressing, and cover art.
- Control playback, scrubbing, volume, queues, MPD random-mode shuffle, and live format information.
- Player status and queue updates travel over one reconnecting WebSocket. If the socket is unavailable, the UI falls back to REST.
- Track actions include play next, clear-and-play, adding to a playlist, and viewing file information.
- Browse MPD playlists and add tracks from the library or a track menu.
- Add, play, and delete persistent HTTP(S) radio stations. Live stream metadata appears in now playing.
- Use CamillaDSP for bit-perfect passthrough, resampling presets, per-device parametric EQ, preamp gain, and AutoEQ-style DSP settings import/export from text files or URLs.
- Enable or disable MPD outputs. Persistent device configuration fragments take effect after an MPD restart.
- On Linux, Bluetooth support covers discovery, pairing, connecting, waking sleeping devices, disconnecting, renaming, removal, and optional A2DP phone input.
- On Linux, the installer provisions an AirPlay receiver through Shairport Sync; iPhone/iPad devices can stream to the server over the LAN.
- Open kiosk mode for full-screen now playing, a real-time FFT visualizer, persistent visualizer settings, and reduced-motion support.
- Install the PWA to a home screen. The service worker updates the app and caches cover art.
- The installer creates an SMB Music share and advertises the web UI, share, and AirPlay receiver through mDNS.

## Install

### Quick install (Debian / Ubuntu / Raspberry Pi OS)

The installer runs on Debian-based Linux systems. It installs MPD, ALSA utilities, CamillaDSP, the Oxide service, BlueZ/BlueALSA, Shairport Sync, Avahi, and Samba. MPD, Bluetooth A2DP, and AirPlay share an ALSA loopback mixer before CamillaDSP, so the same DSP/output chain is used for local and phone playback. It first tries the latest release package for `x86_64` or `arm64`; if no package is available, it builds from source.

Run it as root:

```bash
curl -fsSL https://raw.githubusercontent.com/OxideAI/oxide-player/main/install.sh | sudo bash
```

The installed service listens on `0.0.0.0:80` by default. Open:

```text
http://<server-ip>/
```

If mDNS is available, the service is also reachable at `http://oxide-player.local/`. Set `LISTEN` before running the installer when you need another address:

```bash
curl -fsSL https://raw.githubusercontent.com/OxideAI/oxide-player/main/install.sh \
  | sudo LISTEN=0.0.0.0:8000 bash
```

The main paths and commands are:

```text
Web UI:       http://<server-ip>/
Kiosk:        http://<server-ip>/kiosk
Config:       /etc/oxide-player/config.json
Music share:  smb://<server-ip>/Music
Data/library: /var/lib/oxide-player
Logs:         journalctl -u oxide-player -f
```

Copy music into the `Music` share. Additional folders added under **Settings → Music library sources** are validated, exposed as their own SMB shares, and rescanned automatically; removing a source removes its share. If the library is empty after files were copied with `sudo`, repair ownership and permissions and rescan:

```bash
curl -fsSL https://raw.githubusercontent.com/OxideAI/oxide-player/main/install.sh \
  | sudo bash -s -- --fix-perms
```

Run `sudo bash install.sh --help` to see all installer options. Common environment overrides include `MPD_MUSIC_DIR`, `MPD_CONFIG`, `SAMBA_CONFIG`, `SAMBA_SHARES_CONFIG`, `LISTEN`, `BIN_DIR`, `CONFIG_DIR`, `DATA_DIR`, `CAMILLADSP_CONFIG`, `AIRPLAY_NAME`, `AIRPLAY_CONFIG`, and `SERVICE_USER`.

### iPhone and iPad playback

After installation, both receiver paths use the same CamillaDSP configuration:

**AirPlay (recommended):**

1. Connect the iPhone/iPad and server to the same LAN.
2. Open Control Center, tap the AirPlay output picker, and select the configured `AIRPLAY_NAME` (default: **Oxide Player**).
3. Start playback. Shairport Sync forwards the stream into Oxide's DSP/output path.

**Bluetooth A2DP:**

1. Open **Settings → Devices → Bluetooth** in Oxide and scan.
2. Pair the iPhone when it appears.
3. Tap **Connect**, then enable **A2DP Sink** input.
4. Select **Oxide Player** as the iPhone's Bluetooth audio output and start playback.

Bluetooth input is disabled until its toggle is enabled. AirPlay is advertised automatically by the `oxide-airplay` systemd service. Both inputs feed the shared `oxide_loopback` ALSA PCM; avoid playing unrelated sources simultaneously if you do not want them mixed.

### Upgrade

Use update mode for a normal upgrade. It downloads the latest package for the host architecture, replaces the backend binary and frontend assets, installs or repairs the Bluetooth/AirPlay receiver services and shared ALSA loopback, then restarts Oxide. It leaves `/etc/oxide-player/config.json`, the library database, radio stations, playlists, and DSP data in place:

```bash
curl -fsSL https://raw.githubusercontent.com/OxideAI/oxide-player/main/install.sh \
  | sudo bash -s -- --update
```

The `--update` mode repairs the managed MPD output include and preserves existing MPD/ALSA configuration where possible. Run the full installer when provisioning a host or reapplying all system integration:

```bash
curl -fsSL https://raw.githubusercontent.com/OxideAI/oxide-player/main/install.sh | sudo bash
```

### Uninstall

Remove the Oxide binary, frontend assets, Oxide systemd services, Avahi
advertisement, and installer-managed audio integration:

```bash
curl -fsSL https://raw.githubusercontent.com/OxideAI/oxide-player/main/install.sh \
  | sudo bash -s -- --uninstall
```

For a local checkout:

```bash
sudo bash install.sh --uninstall
```

Uninstall preserves `/etc/oxide-player`, `/var/lib/oxide-player`, and the
configured music directory, including the library database, playlists, radio
stations, and DSP data. It does not remove the service account or Debian
packages. When the installer created `.pre-oxide` backups for MPD or Samba,
uninstall restores them; otherwise those system configuration files are left
unchanged and a warning is printed. Pass the same `DATA_DIR`, `MPD_CONFIG`,
`CONFIG_DIR`, `BIN_DIR`, `SHARE_DIR`, and related overrides used during
installation when uninstalling a custom deployment.

**From source (manual build):**

```bash
git pull
git fetch --tags
tag="$(git describe --tags "$(git rev-list --tags --max-count=1)")"
git checkout "$tag"

cd frontend
npm ci
npm run build
cd ..

cargo build --release
sudo install -m0755 target/release/oxide-player /usr/local/bin/oxide-player
```

For the default installer paths, copy the generated frontend assets before restarting:

```bash
sudo install -d /usr/local/share/oxide-player/dist
sudo cp -r frontend/dist/. /usr/local/share/oxide-player/dist/
sudo systemctl restart oxide-player
```

If `static_dir` points elsewhere, copy `frontend/dist/` there instead. Release archives already include the frontend assets, so `install.sh --update` is the simpler choice unless you need a source build or a custom deployment.

## Quick start (from source)

You need Rust stable, Node 26+, MPD, and CamillaDSP. The automated installer is Linux-only. You can still build the backend and frontend independently on macOS for development.

```bash
# 1. Build the frontend served by the backend
cd frontend
npm ci
npm run build        # tsc + Vite build -> frontend/dist/
cd ..

# 2. Build and run the backend from the repository root
cargo build
./target/debug/oxide-player   # default listen: 127.0.0.1:8000
# flags: --mpd-host <host> --mpd-port <port> --listen <ip:port>
```

For live UI development, run `npm run dev` in `frontend/`. Vite proxies `/api` and WebSocket traffic to the backend. `./dev.sh` is a convenience wrapper for development and production modes.

The backend reads `frontend/dist/` from disk. After a UI-only change, rebuild the frontend; a backend restart is usually unnecessary. New or changed API routes require both a backend rebuild and a restart.

### Config

Without `-c`, Oxide looks for `data/config.json` and otherwise uses its built-in defaults. In a source checkout, start with a config like:

```json
{
  "mpd_host": "127.0.0.1",
  "mpd_port": 6600,
  "listen": "127.0.0.1:8000",
  "data_dir": "./data",
  "library_dirs": ["./music"],
  "static_dir": "./frontend/dist",
  "camilladsp_config_path": "./data/camilladsp/config.yml",
  "camilladsp_ws_url": "ws://127.0.0.1:1234",
  "visualizer_fft": true,
  "visualizer_capture_device": "hw:Loopback,1",
  "visualizer_capture_rate": 44100
}
```

Pass a particular file with `./target/debug/oxide-player -c path/to/config.json`. The installer writes its production config to `/etc/oxide-player/config.json`; keep that file separate from the source example.

## Usage

- Browse albums and tracks, search, open album deep links, refresh the scan, and re-scan cover art.
- Open the queue from the player bar, jump to a track, remove tracks, or toggle MPD random mode.
- Browse saved MPD playlists and add selected tracks from the library.
- Add an HTTP(S) stream in Radio, play it, or remove it. When available, icy metadata appears in now playing.
- Edit CamillaDSP profiles, configure library sources, manage output devices, and manage Bluetooth audio devices on Linux from Settings.
- Open `/kiosk` for the full-screen player and tune the FFT visualizer.
- On a server with an HDMI display attached, the installer sets up a wall-display kiosk: the panel boots straight into `/kiosk` (no login prompt), touch works for walk-up control, and the screen blanks after 10 minutes of stopped playback — playing or paused keeps it lit; a touch wakes it. Disable it with `sudo systemctl disable --now oxide-kiosk`; set the blank timer with `KIOSK_IDLE_SECONDS=<seconds>` at install time.
- Press `?` or `h` to open the keyboard shortcut reference.

## Development

- Run `cargo build` or `cargo test` from the repository root. The binary lands at `target/debug/oxide-player`.
- Run `cd frontend && npm run build` for TypeScript checking and the Vite production build. Run `npm test` for the Vitest suite.
- Frontend styling uses CSS Modules. Tailwind is not used.
- For a bug fix, first write a test that reproduces the bug, then fix it. Keep the regression test in the suite that runs before pushing to `main`.
- MPD 0.24 has no `playid`. Playback uses 0-based queue positions, shuffle is MPD `random` mode, queue removal uses `delete <pos>`, and CUE albums share one URI.

## Documentation

[`ARCHITECTURE.md`](ARCHITECTURE.md) contains:

- System architecture and request flow.
- Backend and frontend module breakdown.
- API route overview and integration notes.
- MPD integration notes and gotchas, including 0-based positions, random-mode shuffle, and CUE handling.
- Build and serve details.

## License

See [`LICENSE`](LICENSE).
