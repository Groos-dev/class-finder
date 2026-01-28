#!/usr/bin/env sh
set -eu

REPO="Groos-dev/class-finder"

version="${VERSION:-}"
install_dir="${INSTALL_DIR:-${HOME}/.local/bin}"

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing dependency: $1" >&2
    exit 1
  }
}

need_cmd curl
need_cmd tar
need_cmd uname

if [ -z "$version" ]; then
  version="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
fi

if [ -z "$version" ]; then
  echo "failed to resolve latest version; set VERSION=v0.0.1-beta" >&2
  exit 1
fi

os="$(uname -s | tr '[:upper:]' '[:lower:]')"
case "$os" in
  darwin) os="macos" ;;
  linux) os="linux" ;;
  *)
    echo "unsupported OS for install.sh: $os" >&2
    echo "use Windows installer: install.ps1" >&2
    exit 1
    ;;
esac

arch="$(uname -m)"
case "$arch" in
  x86_64|amd64) arch="x86_64" ;;
  arm64|aarch64) arch="aarch64" ;;
  *)
    echo "unsupported arch: $arch" >&2
    exit 1
    ;;
esac

asset="class-finder-${os}-${arch}.tar.gz"
url="https://github.com/${REPO}/releases/download/${version}/${asset}"

tmp="${TMPDIR:-/tmp}/class-finder-install.$$"
rm -rf "$tmp"
mkdir -p "$tmp"

echo "Downloading ${url}" >&2
curl -fL -o "$tmp/$asset" "$url"

mkdir -p "$tmp/unpack"
tar -C "$tmp/unpack" -xzf "$tmp/$asset"

mkdir -p "$install_dir"
cp -f "$tmp/unpack/bin/class-finder" "$install_dir/class-finder"
chmod +x "$install_dir/class-finder"

rm -rf "$tmp"

echo "Installed: ${install_dir}/class-finder" >&2
echo "Try: class-finder --help" >&2
