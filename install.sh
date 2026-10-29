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
INSTALL_ROOT="${ISOS_INSTALL_ROOT:-${CARGO_HOME:-${HOME}/.cargo}}"
"$ROOT/build.sh"
"$CARGO_BIN" install --path "$ROOT" --root "$INSTALL_ROOT"
if [[ -n "${ISOS_HOME:-}" ]]; then
  CONFIG_ROOT="$ISOS_HOME/config"
else
  CONFIG_ROOT="${XDG_CONFIG_HOME:-${HOME}/.config}/isos"
fi
mkdir -p "$CONFIG_ROOT"
"$INSTALL_ROOT/bin/isos" --version | awk '{print $2}' > "$CONFIG_ROOT/isos-version"
chmod 600 "$CONFIG_ROOT/isos-version"
COMPLETION_ROOT="${XDG_DATA_HOME:-${HOME}/.local/share}/isos/completions"
mkdir -p "$COMPLETION_ROOT" "${XDG_CONFIG_HOME:-${HOME}/.config}/fish/completions"
"$INSTALL_ROOT/bin/isos" --generate-completion bash > "$COMPLETION_ROOT/isos.bash"
"$INSTALL_ROOT/bin/isos" --generate-completion zsh > "$COMPLETION_ROOT/isos.zsh"
"$INSTALL_ROOT/bin/isos" --generate-completion fish > "${XDG_CONFIG_HOME:-${HOME}/.config}/fish/completions/isos.fish"
if [[ -f "$HOME/.bashrc" || "${SHELL:-}" == */bash ]]; then
  touch "$HOME/.bashrc"
  if ! grep -Fq '# IsOS completion' "$HOME/.bashrc"; then
    printf '\n# IsOS completion\n[[ -f "%s/isos.bash" ]] && source "%s/isos.bash"\n' "$COMPLETION_ROOT" "$COMPLETION_ROOT" >> "$HOME/.bashrc"
  fi
fi
if [[ -f "$HOME/.zshrc" || "${SHELL:-}" == */zsh ]]; then
  touch "$HOME/.zshrc"
  if ! grep -Fq '# IsOS completion' "$HOME/.zshrc"; then
    printf '\n# IsOS completion\nif [[ -f "%s/isos.zsh" ]]; then\n  (( $+functions[compdef] )) || { autoload -Uz compinit && compinit; }\n  source "%s/isos.zsh"\nfi\n' "$COMPLETION_ROOT" "$COMPLETION_ROOT" >> "$HOME/.zshrc"
  fi
fi
echo "Installed IsOS shell completions; open a new shell to enable Tab completion."
if ! command -v isos >/dev/null 2>&1; then
  case ":$PATH:" in
    *":${HOME}/.local/bin:"*)
      mkdir -p "$HOME/.local/bin"
      if [[ ! -e "$HOME/.local/bin/isos" && ! -L "$HOME/.local/bin/isos" ]]; then
        ln -s "$INSTALL_ROOT/bin/isos" "$HOME/.local/bin/isos"
        echo "Linked IsOS at $HOME/.local/bin/isos"
      fi
      ;;
  esac
fi
if ! command -v isos >/dev/null 2>&1; then
  echo "Add $INSTALL_ROOT/bin to PATH, then open a new shell to run isos." >&2
fi
