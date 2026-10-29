#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if command -v cargo >/dev/null 2>&1; then
  CARGO_BIN="$(command -v cargo)"
elif [[ -x "${HOME:-}/.cargo/bin/cargo" ]]; then
  CARGO_BIN="${HOME}/.cargo/bin/cargo"
else
  echo "IsOS requires Rust Cargo; install Rust from https://rustup.rs or add ~/.cargo/bin to PATH" >&2
  exit 127
fi
"$CARGO_BIN" install --path "$ROOT"
if [[ -n "${ISOS_HOME:-}" ]]; then
  CONFIG_ROOT="$ISOS_HOME/config"
else
  CONFIG_ROOT="${XDG_CONFIG_HOME:-${HOME}/.config}/isos"
fi
mkdir -p "$CONFIG_ROOT"
"${CARGO_HOME:-${HOME}/.cargo}/bin/isos" --version | awk '{print $2}' > "$CONFIG_ROOT/isos-version"
chmod 600 "$CONFIG_ROOT/isos-version"
if [[ ! -f "$ROOT/target/isos-ui/index.html" ]]; then
  "$ROOT/build.sh"
fi
if [[ -n "${ISOS_HOME:-}" ]]; then
  DATA_ROOT="$ISOS_HOME/data/ui"
else
  DATA_ROOT="${XDG_DATA_HOME:-${HOME}/.local/share}/isos/ui"
fi
mkdir -p "$DATA_ROOT"
cp -a "$ROOT/target/isos-ui/." "$DATA_ROOT/"
echo "Installed IsOS configuration UI at $DATA_ROOT"
