#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo build --release

os="macos"
arch="$(uname -m)"

dist_dir="${repo_root}/dist"
rm -rf "${dist_dir}/package"
mkdir -p "${dist_dir}/package/bin"

cp -f "target/release/class-finder" "${dist_dir}/package/bin/class-finder"
cp -f "README.md" "${dist_dir}/package/README.md"
cp -f "LICENSE" "${dist_dir}/package/LICENSE"

tarball="${dist_dir}/class-finder-${os}-${arch}.tar.gz"
tar -C "${dist_dir}/package" -czf "$tarball" .

echo "Created: $tarball"
