#!/bin/sh
set -eu

repo="${GREENTIC_DESKTOP_REPO:-greenticai/greentic-desktop}"
version="${GREENTIC_DESKTOP_VERSION:-latest}"
install_dir="${GREENTIC_DESKTOP_INSTALL_DIR:-$HOME/.greentic/desktop/bin}"
bin_dir="${GREENTIC_DESKTOP_BIN_DIR:-$HOME/.local/bin}"

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'greentic-desktop installer requires %s.\n' "$1" >&2
    exit 1
  fi
}

need uname
need curl
need tar
need mktemp
need mkdir
need cp
need chmod
need find
need ln
need rm
need basename
need sed
need head

os="$(uname -s)"
arch="$(uname -m)"
case "$os:$arch" in
  Darwin:x86_64) target="x86_64-apple-darwin"; ext="tgz" ;;
  Darwin:arm64) target="aarch64-apple-darwin"; ext="tgz" ;;
  Linux:x86_64) target="x86_64-unknown-linux-gnu"; ext="tgz" ;;
  Linux:aarch64|Linux:arm64) target="aarch64-unknown-linux-gnu"; ext="tgz" ;;
  *)
    printf 'Unsupported OS/architecture: %s %s\n' "$os" "$arch" >&2
    exit 1
    ;;
esac

api_base="https://api.github.com/repos/$repo/releases"
if [ "$version" = "latest" ]; then
  release_url="$api_base/latest"
else
  release_url="$api_base/tags/$version"
fi

tmp="$(mktemp -d "${TMPDIR:-/tmp}/greentic-desktop-install.XXXXXX")"
cleanup() {
  rm -rf "$tmp"
}
trap cleanup EXIT INT TERM

json="$tmp/release.json"
printf 'Resolving Greentic Desktop release for %s...\n' "$target"
curl -fsSL "$release_url" -o "$json"

asset_url="$(sed -n 's/.*"browser_download_url":[[:space:]]*"\([^"]*greentic-desktop-v[^"]*-'$target'\.'$ext'\)".*/\1/p' "$json" | head -n 1)"
if [ -z "$asset_url" ]; then
  printf 'No Greentic Desktop release asset found for target %s in %s.\n' "$target" "$repo" >&2
  exit 1
fi

archive="$tmp/greentic-desktop.$ext"
printf 'Downloading %s...\n' "$(basename "$asset_url")"
curl -fL "$asset_url" -o "$archive"

checksum_url="$(sed -n 's/.*"browser_download_url":[[:space:]]*"\([^"]*checksums.txt\)".*/\1/p' "$json" | head -n 1)"
if [ -n "$checksum_url" ]; then
  checksums="$tmp/checksums.txt"
  curl -fsSL "$checksum_url" -o "$checksums"
  archive_name="$(basename "$asset_url")"
  expected="$(sed -n "s/^\([0-9a-fA-F][0-9a-fA-F]*\)[[:space:]][[:space:]]*[*]*${archive_name}\$/\1/p" "$checksums" | head -n 1)"
  if [ -z "$expected" ]; then
    printf 'checksums.txt did not contain %s; skipping checksum verification.\n' "$archive_name" >&2
  elif command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$archive" | sed 's/[[:space:]].*//')"
    if [ "$actual" != "$expected" ]; then
      printf 'Checksum verification failed for %s.\n' "$archive_name" >&2
      exit 1
    fi
  elif command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$archive" | sed 's/[[:space:]].*//')"
    if [ "$actual" != "$expected" ]; then
      printf 'Checksum verification failed for %s.\n' "$archive_name" >&2
      exit 1
    fi
  else
    printf 'checksums.txt is available but no sha256sum or shasum command was found; skipping verification.\n' >&2
  fi
else
  printf 'checksums.txt was not found on the release; continuing without checksum verification.\n' >&2
fi

extract_dir="$tmp/extract"
mkdir -p "$extract_dir"
tar -xzf "$archive" -C "$extract_dir"

desktop_bin="$(find "$extract_dir" -type f -name greentic-desktop -perm -u+x 2>/dev/null | head -n 1 || true)"
gtc_bin="$(find "$extract_dir" -type f -name gtc -perm -u+x 2>/dev/null | head -n 1 || true)"
if [ -z "$desktop_bin" ]; then
  desktop_bin="$(find "$extract_dir" -type f -name greentic-desktop | head -n 1 || true)"
fi
if [ -z "$gtc_bin" ]; then
  gtc_bin="$(find "$extract_dir" -type f -name gtc | head -n 1 || true)"
fi
if [ -z "$desktop_bin" ] || [ -z "$gtc_bin" ]; then
  printf 'Release archive did not contain greentic-desktop and gtc binaries.\n' >&2
  exit 1
fi

mkdir -p "$install_dir" "$bin_dir"
cp "$desktop_bin" "$install_dir/greentic-desktop"
cp "$gtc_bin" "$install_dir/gtc"
chmod +x "$install_dir/greentic-desktop" "$install_dir/gtc"

ln -sfn "$install_dir/greentic-desktop" "$bin_dir/greentic-desktop"
ln -sfn "$install_dir/gtc" "$bin_dir/gtc"

if [ "${GREENTIC_DESKTOP_NO_INIT:-0}" != "1" ]; then
  "$install_dir/greentic-desktop" init
fi

printf '\nGreentic Desktop installed successfully.\n'
printf 'Installed binaries: %s\n' "$install_dir"
printf 'Command shims:      %s\n' "$bin_dir"

case ":$PATH:" in
  *":$bin_dir:"*) ;;
  *)
    printf '\n%s is not currently on PATH. Add this to your shell profile:\n' "$bin_dir"
    printf '  export PATH="%s:$PATH"\n' "$bin_dir"
    ;;
esac

printf '\nTry:\n'
printf '  greentic-desktop\n'
printf '  gtc desktop info\n'
