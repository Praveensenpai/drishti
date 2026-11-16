#!/usr/bin/env bash
# Drishti installer — curl -fsSL <url> | bash
set -euo pipefail

BIN_DIR="${DRISHTI_BIN:-$HOME/.local/bin}"
REPO="${DRISHTI_REPO:-https://github.com/Praveensenpai/drishti.git}"
SRC="${DRISHTI_SRC:-$HOME/.local/share/drishti/src}"
CONFIG_DIR="$HOME/.config/drishti"
RELEASE_URL="https://github.com/Praveensenpai/drishti/releases/latest/download/drishti-linux-x86_64"

path_hint() {
  case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo; echo "!! Add to your PATH:"; echo "   export PATH=\"\$PATH:$BIN_DIR\""; echo ;;
  esac
}

echo "==> Drishti installer"

# 1. Prefer the prebuilt binary (no Rust needed) -----------------------------
mkdir -p "$BIN_DIR"
if curl -fsSL "$RELEASE_URL" -o "$BIN_DIR/drishti.tmp" 2>/dev/null; then
  chmod +x "$BIN_DIR/drishti.tmp"
  mv "$BIN_DIR/drishti.tmp" "$BIN_DIR/drishti"
  echo "==> installed prebuilt binary to $BIN_DIR/drishti"
  path_hint
  echo "==> Done!  Run:  drishti"
  exit 0
fi

# 2. Fallback: build from source --------------------------------------------
echo "==> No prebuilt binary found; building from source…"
if ! command -v cargo >/dev/null 2>&1; then
  echo "==> Installing Rust (rustup)…"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1090
  . "$HOME/.cargo/env"
fi
if [ -d "$SRC/.git" ]; then
  echo "==> Updating source in $SRC"
  git -C "$SRC" pull --ff-only
else
  echo "==> Cloning $REPO"
  mkdir -p "$(dirname "$SRC")"
  git clone --depth 1 "$REPO" "$SRC"
fi
echo "==> Building & installing drishti…"
cargo install --path "$SRC" --force
mkdir -p "$CONFIG_DIR"
printf '%s\n' "$SRC" > "$CONFIG_DIR/source.txt"
path_hint
echo "==> Done!  Run:  drishti"
