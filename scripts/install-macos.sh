#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$repo_root"
cargo build --release

install_dir="${HOME}/.local/bin"
mkdir -p "$install_dir"
cp -f "target/release/class-finder" "${install_dir}/class-finder"

echo "Installed: ${install_dir}/class-finder"
echo "If needed, add to PATH: export PATH=\"${HOME}/.local/bin:$PATH\""
