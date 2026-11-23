#!/usr/bin/env bash
# Uninstall preserves an existing physical music volume and its boot entry.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

export OXIDE_INSTALLER_TEST=1
export BIN_DIR="$tmp_dir/bin"
export SHARE_DIR="$tmp_dir/share"
export CONFIG_DIR="$tmp_dir/config"
export DATA_DIR="$tmp_dir/data"
export SYSTEMD_DIR="$tmp_dir/systemd"
export MPD_MUSIC_DIR="$tmp_dir/music"
export MPD_CONFIG="$tmp_dir/mpd.conf"
export SAMBA_CONFIG="$tmp_dir/smb.conf"
export ASOUND_CONFIG="$tmp_dir/asound.conf"
export AIRPLAY_CONFIG="$tmp_dir/airplay.conf"
export SUDOERS_RULE="$tmp_dir/sudoers"
export FSTAB_FILE="$tmp_dir/fstab"

mkdir -p "$CONFIG_DIR" "$MPD_MUSIC_DIR"
printf 'music remains\n' > "$MPD_MUSIC_DIR/song.flac"
printf 'UUID=root / ext4 defaults 0 1\nUUID=music %s ext4 defaults 0 2\n' \
  "$MPD_MUSIC_DIR" > "$FSTAB_FILE"
cp "$FSTAB_FILE" "$tmp_dir/expected-fstab"

source "$repo_root/install.sh"
need_root() { :; }
check_linux() { :; }
systemctl() { :; }
loginctl() { :; }
udevadm() { :; }
mounted=true
unmount_fails=false
mountpoint() { $mounted && [ "${@: -1}" = "$MPD_MUSIC_DIR" ]; }
umount() {
  printf '%s\n' "$*" >> "$tmp_dir/unmounts"
  ! $unmount_fails
}
# do_uninstall also considers fixed system integration paths. Keep every
# filesystem deletion in the fixture, including when tests run on Linux.
remove_path() {
  case "$1" in
    "$tmp_dir"/*) rm -rf -- "$1" ;;
  esac
}

do_uninstall

cmp "$FSTAB_FILE" "$tmp_dir/expected-fstab"
test ! -e "$tmp_dir/unmounts"
test -f "$MPD_MUSIC_DIR/song.flac"
printf 'uninstall preserves user music mounts\n'

# Old installer binds have no ownership marker. Do not adopt or remove them.
printf 'UUID=root / ext4 defaults 0 1\n/source %s none bind,nofail 0 0\n' \
  "$MPD_MUSIC_DIR" > "$FSTAB_FILE"
cp "$FSTAB_FILE" "$tmp_dir/expected-fstab"
do_uninstall > "$tmp_dir/uninstall.log" 2>&1
cmp "$FSTAB_FILE" "$tmp_dir/expected-fstab"
test ! -e "$tmp_dir/unmounts"

# A marked bind beside a user entry is ambiguous and authorizes no cleanup.
printf '/another-source %s none bind 0 0 # oxide-player: managed music bind\n' \
  "$MPD_MUSIC_DIR" >> "$FSTAB_FILE"
cp "$FSTAB_FILE" "$tmp_dir/expected-fstab"
do_uninstall > "$tmp_dir/uninstall.log" 2>&1
cmp "$FSTAB_FILE" "$tmp_dir/expected-fstab"
test ! -e "$tmp_dir/unmounts"

# An unmounted bind created by this installer can be removed without touching
# the unrelated root entry, and repeated uninstall remains a no-op.
printf 'UUID=root / ext4 defaults 0 1\n' > "$FSTAB_FILE"
printf '/source %s none bind,nofail 0 0 # oxide-player: managed music bind\n' \
  "$MPD_MUSIC_DIR" >> "$FSTAB_FILE"
mounted=false
do_uninstall > "$tmp_dir/uninstall.log" 2>&1
printf 'UUID=root / ext4 defaults 0 1\n' > "$tmp_dir/expected-fstab"
cmp "$FSTAB_FILE" "$tmp_dir/expected-fstab"
test ! -e "$tmp_dir/unmounts"
do_uninstall > "$tmp_dir/uninstall.log" 2>&1
cmp "$FSTAB_FILE" "$tmp_dir/expected-fstab"

# Model the matching inode/device identity of a bind using a fixture symlink;
# no mount syscall is needed. Include spaces in the source path.
source_dir="$tmp_dir/source volume"
mv "$MPD_MUSIC_DIR" "$source_dir"
ln -s "$source_dir" "$MPD_MUSIC_DIR"
printf 'UUID=root / ext4 defaults 0 1\n' > "$FSTAB_FILE"
printf '%s %s none bind,nofail 0 0 # oxide-player: managed music bind\n' \
  "${source_dir// /\\040}" "$MPD_MUSIC_DIR" >> "$FSTAB_FILE"
cp "$FSTAB_FILE" "$tmp_dir/expected-fstab"
mounted=true
unmount_fails=true
do_uninstall > "$tmp_dir/uninstall.log" 2>&1
cmp "$FSTAB_FILE" "$tmp_dir/expected-fstab"
test -s "$tmp_dir/unmounts"
rm "$tmp_dir/unmounts"
unmount_fails=false
do_uninstall > "$tmp_dir/uninstall.log" 2>&1
printf 'UUID=root / ext4 defaults 0 1\n' > "$tmp_dir/expected-fstab"
cmp "$FSTAB_FILE" "$tmp_dir/expected-fstab"
test "$(wc -l < "$tmp_dir/unmounts" | tr -d ' ')" -eq 1
test -f "$source_dir/song.flac"

# A stale owned entry must not authorize unmounting a different volume.
rm "$tmp_dir/unmounts"
printf 'UUID=root / ext4 defaults 0 1\n/wrong-source %s none bind 0 0 # oxide-player: managed music bind\n' \
  "$MPD_MUSIC_DIR" > "$FSTAB_FILE"
cp "$FSTAB_FILE" "$tmp_dir/expected-fstab"
do_uninstall > "$tmp_dir/uninstall.log" 2>&1
cmp "$FSTAB_FILE" "$tmp_dir/expected-fstab"
test ! -e "$tmp_dir/unmounts"
printf 'managed music bind cleanup tests passed\n'

mountpoint() {
  if [ "${@: -1}" = "$source_dir" ]; then return 0; fi
  $mounted && [ "${@: -1}" = "$MPD_MUSIC_DIR" ]
}
mount() {
  printf '%s\n' "$*" >> "$tmp_dir/mounts"
  rmdir "$MPD_MUSIC_DIR"
  ln -s "$source_dir" "$MPD_MUSIC_DIR"
  mounted=true
}
printf '{"library_dirs":["%s"]}\n' "$source_dir" > "$CONFIG_DIR/config.json"
rm "$MPD_MUSIC_DIR"
mkdir "$MPD_MUSIC_DIR"
mounted=false
printf 'UUID=root / ext4 defaults 0 1\nUUID=music %s ext4 defaults 0 2\n' \
  "$MPD_MUSIC_DIR" > "$FSTAB_FILE"
cp "$FSTAB_FILE" "$tmp_dir/expected-fstab"
setup_music_bind > "$tmp_dir/setup.log" 2>&1
cmp "$FSTAB_FILE" "$tmp_dir/expected-fstab"
test ! -e "$tmp_dir/mounts"

printf 'UUID=root / ext4 defaults 0 1\n' > "$FSTAB_FILE"
setup_music_bind > "$tmp_dir/setup.log" 2>&1
grep -Fq '# oxide-player: managed music bind' "$FSTAB_FILE"
test -s "$tmp_dir/mounts"
cp "$FSTAB_FILE" "$tmp_dir/expected-fstab"
setup_music_bind > "$tmp_dir/setup.log" 2>&1
cmp "$FSTAB_FILE" "$tmp_dir/expected-fstab"
test "$(wc -l < "$tmp_dir/mounts" | tr -d ' ')" -eq 1
do_uninstall > "$tmp_dir/uninstall.log" 2>&1
test "$(wc -l < "$FSTAB_FILE" | tr -d ' ')" -eq 1
test -f "$source_dir/song.flac"
printf 'managed music bind setup tests passed\n'
