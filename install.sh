#!/usr/bin/env bash
# Drishti installer — curl -fsSL <url> | bash
set -euo pipefail

REPO="${DRISHTI_REPO:-https://github.com/Praveensenpai/drishti.git}"
SRC="${DRISHTI_SRC:-$HOME/.local/share/drishti/src}"
CONFIG_DIR="$HOME/.config/drishti"

echo "==> Drishti installer"

# 1. Rust toolchain ---------------------------------------------------------
if ! command -v cargo >/dev/null 2>&1; then
  echo "==> Installing Rust (rustup)…"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1090
  . "$HOME/.cargo/env"
fi

# 2. Source -----------------------------------------------------------------
if [ -d "$SRC/.git" ]; then
  echo "==> Updating source in $SRC"
  git -C "$SRC" pull --ff-only
else
  echo "==> Cloning $REPO"
  mkdir -p "$(dirname "$SRC")"
  git clone --depth 1 "$REPO" "$SRC"
fi

# 3. Build & install --------------------------------------------------------
echo "==> Building & installing drishti (this may take a minute)…"
cargo install --path "$SRC" --force

# 4. Record source so `drishti update` works -------------------------------
mkdir -p "$CONFIG_DIR"
printf '%s\n' "$SRC" > "$CONFIG_DIR/source.txt"

# 5. PATH hint --------------------------------------------------------------
case ":$PATH:" in
  *":$HOME/.cargo/bin:"*) ;;
  *)
    echo
    echo "!! Add Rust's bin to your PATH:"
    echo "   export PATH=\"\$HOME/.cargo/bin:\$PATH\""
    echo
    ;;
esac

echo "==> Done!  Run:  drishti"