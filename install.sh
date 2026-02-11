#!/usr/bin/env sh
set -eu

REPO="Groos-dev/class-finder"

version="${VERSION:-}"
allow_prerelease="${ALLOW_PRERELEASE:-}"
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

fetch_json() {
  curl -fsSL "$1"
}

pick_latest_tag() {
  api="https://api.github.com/repos/${REPO}/releases?per_page=20"
  tags="$(fetch_json "$api" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')"

  first=""
  for t in $tags; do
    [ -z "$t" ] && continue
    if [ -z "$first" ]; then
      first="$t"
    fi

    case "$t" in
      *beta*|*alpha*|*rc*) : ;;
      *)
        printf '%s\n' "$t"
        return 0
        ;;
    esac
  done

  if [ -n "$first" ]; then
    printf '%s\n' "$first"
    return 0
  fi

  return 1
}

pick_latest_tag_allow_prerelease() {
  api="https://api.github.com/repos/${REPO}/releases?per_page=1"
  fetch_json "$api" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1
}

if [ -z "$version" ]; then
  if [ "${allow_prerelease}" = "1" ]; then
    version="$(pick_latest_tag_allow_prerelease || true)"
  else
    version="$(pick_latest_tag || true)"
  fi
fi

if [ -z "$version" ]; then
  echo "failed to resolve version; set VERSION=v0.0.1-beta.2" >&2
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
sum_url="https://github.com/${REPO}/releases/download/${version}/SHA256SUMS"

tmp="${TMPDIR:-/tmp}/class-finder-install.$$"
rm -rf "$tmp"
mkdir -p "$tmp"

class_finder_home=""
if [ "$os" = "macos" ]; then
  class_finder_home="${HOME}/Library/Application Support/class-finder"
else
  class_finder_home="${XDG_DATA_HOME:-${HOME}/.local/share}/class-finder"
fi

echo "Downloading ${url}" >&2
curl -fL -o "$tmp/$asset" "$url"

echo "Verifying SHA256 (${sum_url})" >&2
curl -fL -o "$tmp/SHA256SUMS" "$sum_url"
expected="$(grep "  ${asset}\$" "$tmp/SHA256SUMS" | head -n 1 | awk '{print $1}')"
if [ -z "$expected" ]; then
  echo "missing checksum for ${asset} in SHA256SUMS" >&2
  exit 1
fi

actual=""
if command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "$tmp/$asset" | awk '{print $1}')"
elif command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp/$asset" | awk '{print $1}')"
else
  echo "missing dependency: shasum or sha256sum" >&2
  exit 1
fi

if [ "$actual" != "$expected" ]; then
  echo "SHA256 mismatch for ${asset}" >&2
  echo "expected: ${expected}" >&2
  echo "actual:   ${actual}" >&2
  exit 1
fi

mkdir -p "$tmp/unpack"
tar -C "$tmp/unpack" -xzf "$tmp/$asset"

mkdir -p "$install_dir"
cp -f "$tmp/unpack/bin/class-finder" "$install_dir/class-finder"
chmod +x "$install_dir/class-finder"

default_cfr_url="https://github.com/leibnitz27/cfr/releases/download/0.152/cfr-0.152.jar"
cfr_url="${CFR_URL:-$default_cfr_url}"
cfr_dir="${class_finder_home}/tools"
cfr_path="${cfr_dir}/cfr.jar"

if [ ! -f "$cfr_path" ]; then
  echo "Downloading CFR (${cfr_url})" >&2
  mkdir -p "$cfr_dir"
  curl -fL -o "$tmp/cfr.jar" "$cfr_url"
  mv -f "$tmp/cfr.jar" "$cfr_path"
fi

claude_skill_dir="${HOME}/.claude/skills/find-class"
mkdir -p "$claude_skill_dir"
skill_ref="${SKILL_REF:-$version}"
skill_url="https://raw.githubusercontent.com/${REPO}/${skill_ref}/skills/find-class/SKILL.md"
skill_fallback_url="https://raw.githubusercontent.com/${REPO}/main/skills/find-class/SKILL.md"

echo "Installing Claude skill to ${claude_skill_dir}/SKILL.md" >&2
if ! curl -fsSL -o "${claude_skill_dir}/SKILL.md" "$skill_url"; then
  curl -fsSL -o "${claude_skill_dir}/SKILL.md" "$skill_fallback_url"
fi

rm -rf "$tmp"

echo "Installed: ${install_dir}/class-finder" >&2
echo "Installed: ${claude_skill_dir}/SKILL.md" >&2
echo "Try: class-finder --help" >&2
