#!/usr/bin/env bash
# photoshow — instalador one-line (Linux x86_64, sem root).
#
#   curl -fsSL https://raw.githubusercontent.com/raillen/photoshow/main/install.sh | bash
#
# Variáveis:
#   PHOTOSHOW_VERSION=v0.1.0-rc1  trava uma versão (padrão: última release)
#   BIN_DIR=$HOME/.local/bin      destino do binário
set -euo pipefail

REPO="raillen/photoshow"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
APP_DIR="$HOME/.local/share/applications"
ICON_DIR="$HOME/.local/share/icons/hicolor/scalable/apps"

echo "==> photoshow: instalando"

if [ -z "${PHOTOSHOW_VERSION:-}" ]; then
  echo "--> descobrindo última release…"
  PHOTOSHOW_VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep -m1 '"tag_name"' | cut -d'"' -f4)"
fi
echo "--> versão: $PHOTOSHOW_VERSION"

TARBALL="photoshow-$PHOTOSHOW_VERSION-x86_64-unknown-linux-gnu.tar.gz"
URL="https://github.com/$REPO/releases/download/$PHOTOSHOW_VERSION/$TARBALL"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
echo "--> baixando $URL"
curl -fsSL --retry 3 -o "$TMP/photoshow.tar.gz" "$URL"
tar xzf "$TMP/photoshow.tar.gz" -C "$TMP"

install -Dm755 "$TMP/photoshow" "$BIN_DIR/photoshow"
install -Dm644 "$TMP/photoshow.desktop" "$APP_DIR/photoshow.desktop"
install -Dm644 "$TMP/photoshow.svg" "$ICON_DIR/photoshow.svg" 2>/dev/null || true

# Aponta o .desktop para o binário instalado (tarball usa nome genérico).
sed -i "s|^Exec=.*|Exec=$BIN_DIR/photoshow %F|" "$APP_DIR/photoshow.desktop"
sed -i "s|^Icon=.*|Icon=photoshow|" "$APP_DIR/photoshow.desktop"

command -v update-desktop-database >/dev/null && update-desktop-database "$APP_DIR" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$HOME/.local/share/icons/hicolor" || true

echo "==> pronto! rode com: photoshow"
echo "    (se 'photoshow' não for encontrado, adicione ~/.local/bin ao PATH)"
