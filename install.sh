#!/usr/bin/env bash
# photoshow — instalador one-line (Linux x86_64, sem root).
#
#   curl -fsSL https://raw.githubusercontent.com/wasd-lat/photoshow/main/install.sh | bash
#
# Variáveis:
#   PHOTOSHOW_VERSION=v0.1.0-rc1  trava uma versão (padrão: última release)
#   PHOTOSHOW_ARCHIVE=/path/file  usa um tarball local (smoke tests/offline)
#   BIN_DIR=$HOME/.local/bin      destino do binário
set -euo pipefail

REPO="wasd-lat/photoshow"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
APP_DIR="$HOME/.local/share/applications"
ICON_DIR="$HOME/.local/share/icons/hicolor/scalable/apps"

echo "==> photoshow: instalando"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if [ -n "${PHOTOSHOW_ARCHIVE:-}" ]; then
  ARCHIVE="${PHOTOSHOW_ARCHIVE}"
  if [ ! -f "$ARCHIVE" ]; then
    echo "erro: PHOTOSHOW_ARCHIVE não existe: $ARCHIVE" >&2
    exit 1
  fi
  PHOTOSHOW_VERSION="${PHOTOSHOW_VERSION:-local}"
  TARBALL="$(basename "$ARCHIVE")"
  cp "$ARCHIVE" "$TMP/photoshow.tar.gz"
  echo "--> usando arquivo local: $ARCHIVE"
else
  if [ -z "${PHOTOSHOW_VERSION:-}" ]; then
    echo "--> descobrindo última release…"
    PHOTOSHOW_VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
      | grep -m1 '"tag_name"' | cut -d'"' -f4)"
  fi
  echo "--> versão: $PHOTOSHOW_VERSION"

  TARBALL="photoshow-$PHOTOSHOW_VERSION-x86_64-unknown-linux-gnu.tar.gz"
  URL="https://github.com/$REPO/releases/download/$PHOTOSHOW_VERSION/$TARBALL"
  echo "--> baixando $URL"
  curl -fsSL --retry 3 -o "$TMP/photoshow.tar.gz" "$URL"

  CHECKSUM_URL="https://github.com/$REPO/releases/download/$PHOTOSHOW_VERSION/SHA256SUMS.txt"
  if curl -fsSL --retry 3 -o "$TMP/SHA256SUMS.txt" "$CHECKSUM_URL"; then
    expected="$(awk -v file="$TARBALL" '$2 == file || $2 == "*" file { print $1; exit }' "$TMP/SHA256SUMS.txt")"
    if [ -z "$expected" ]; then
      echo "erro: checksum de $TARBALL não encontrado em SHA256SUMS.txt" >&2
      exit 1
    fi
    actual="$(sha256sum "$TMP/photoshow.tar.gz" | awk '{print $1}')"
    if [ "$actual" != "$expected" ]; then
      echo "erro: checksum inválido para $TARBALL" >&2
      exit 1
    fi
    echo "--> checksum SHA-256 verificado"
  else
    echo "aviso: release sem SHA256SUMS.txt; prosseguindo por compatibilidade" >&2
  fi
fi

EXTRACT="$TMP/extract"
mkdir -p "$EXTRACT"
tar xzf "$TMP/photoshow.tar.gz" -C "$EXTRACT"

# Releases novas têm layout flat. Para arquivos antigos, aceite um único
# diretório de staging sem tornar a detecção permissiva demais.
ROOT="$EXTRACT"
if [ ! -f "$ROOT/photoshow" ]; then
  candidate="$(find "$EXTRACT" -mindepth 1 -maxdepth 2 -type f -name photoshow -print -quit)"
  if [ -n "$candidate" ]; then
    ROOT="$(dirname "$candidate")"
  fi
fi

for required in photoshow photoshow.desktop photoshow.svg; do
  if [ ! -f "$ROOT/$required" ]; then
    echo "erro: pacote inválido; faltando $required" >&2
    exit 1
  fi
done

install -Dm755 "$ROOT/photoshow" "$BIN_DIR/photoshow"
install -Dm644 "$ROOT/photoshow.desktop" "$APP_DIR/photoshow.desktop"
install -Dm644 "$ROOT/photoshow.svg" "$ICON_DIR/photoshow.svg"

# Aponta o .desktop para o binário instalado.
sed -i "s|^Exec=.*|Exec=$BIN_DIR/photoshow %F|" "$APP_DIR/photoshow.desktop"
sed -i "s|^Icon=.*|Icon=photoshow|" "$APP_DIR/photoshow.desktop"

command -v update-desktop-database >/dev/null && update-desktop-database "$APP_DIR" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$HOME/.local/share/icons/hicolor" || true

echo "==> pronto! rode com: photoshow"
echo "    (se 'photoshow' não for encontrado, adicione ~/.local/bin ao PATH)"
