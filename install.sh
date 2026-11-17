#!/usr/bin/env bash
# Drushti installer — curl -fsSL <url> | bash
set -euo pipefail

BIN_DIR="${DRUSHTI_BIN:-$HOME/.local/bin}"
REPO="${DRUSHTI_REPO:-https://github.com/Praveensenpai/drushti.git}"
SRC="${DRUSHTI_SRC:-$HOME/.local/share/drushti/src}"
RELEASE_URL="https://github.com/Praveensenpai/drushti/releases/latest/download/drushti-linux-x86_64"

path_hint() {
  case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo; echo "!! Add to your PATH:"; echo "   export PATH=\"\$PATH:$BIN_DIR\""; echo ;;
  esac
}

echo "==> Drushti installer"

# 1. Prefer the prebuilt binary (no Rust needed) -----------------------------
mkdir -p "$BIN_DIR"
if curl -fsSL "$RELEASE_URL" -o "$BIN_DIR/drushti.tmp" 2>/dev/null; then
  chmod +x "$BIN_DIR/drushti.tmp"
  mv "$BIN_DIR/drushti.tmp" "$BIN_DIR/drushti"
  echo "==> installed prebuilt binary to $BIN_DIR/drushti"
  path_hint
  echo "==> Done!  Run:  drushti"
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
echo "==> Building & installing drushti…"
cargo install --path "$SRC" --force
path_hint
echo "==> Done!  Run:  drushti"
