#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"
if command -v cargo >/dev/null 2>&1; then
  CARGO_BIN="$(command -v cargo)"
elif [[ -x "${HOME:-}/.cargo/bin/cargo" ]]; then
  CARGO_BIN="${HOME}/.cargo/bin/cargo"
else
  echo "IsOS requires Rust Cargo; install Rust from https://rustup.rs or add ~/.cargo/bin to PATH" >&2
  exit 127
fi
"$CARGO_BIN" build --release
WEB_BUNDLE="${ISOS_WEB_BUNDLE:-$ROOT/../Island-web/dist/isos}"
if [[ ! -f "$WEB_BUNDLE/index.html" ]]; then
  if [[ -z "${ISOS_WEB_BUNDLE:-}" && -f "$ROOT/../Island-web/package.json" ]]; then
    (cd "$ROOT/../Island-web" && npm run build)
  fi
fi
[[ -f "$WEB_BUNDLE/index.html" ]] || { echo "IsOS web bundle missing: $WEB_BUNDLE" >&2; exit 1; }
mkdir -p "$ROOT/target/isos-ui"
cp -a "$WEB_BUNDLE/." "$ROOT/target/isos-ui/"
