#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: ./install.sh [--prefix DIR] [--uninstall]

Installs Engram for the current user (default prefix: ~/.local) and registers
the .engram file type. Use --prefix /usr/local with sudo for a system install.
USAGE
}

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX="$HOME/.local"
UNINSTALL=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix) PREFIX="$2"; shift 2 ;;
    --uninstall) UNINSTALL=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

APP_ID=com.liwidale.engram
BIN="$PREFIX/bin"
SHARE="$PREFIX/share"
SIZES=(32 48 64 128 256 512)

put() {
  mkdir -p "$(dirname "$3")"
  install -m "$1" "$2" "$3"
}

refresh() {
  command -v update-mime-database >/dev/null && update-mime-database "$SHARE/mime" || true
  command -v update-desktop-database >/dev/null && update-desktop-database "$SHARE/applications" || true
  command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$SHARE/icons/hicolor" || true
}

if [[ $UNINSTALL -eq 1 ]]; then
  rm -f "$BIN/engram" "$BIN/engram-mcp" "$SHARE/applications/$APP_ID.desktop" "$SHARE/mime/packages/$APP_ID.xml"
  for s in "${SIZES[@]}"; do rm -f "$SHARE/icons/hicolor/${s}x${s}/apps/$APP_ID.png"; done
  refresh
  echo "Engram was removed from $PREFIX"
  exit 0
fi

put 755 "$HERE/engram" "$BIN/engram"
put 755 "$HERE/engram-mcp" "$BIN/engram-mcp"
put 644 "$HERE/$APP_ID.desktop" "$SHARE/applications/$APP_ID.desktop"
put 644 "$HERE/$APP_ID.xml" "$SHARE/mime/packages/$APP_ID.xml"
for s in "${SIZES[@]}"; do
  put 644 "$HERE/icons/engram-$s.png" "$SHARE/icons/hicolor/${s}x${s}/apps/$APP_ID.png"
done
refresh
command -v xdg-mime >/dev/null && xdg-mime default "$APP_ID.desktop" application/vnd.engram+json || true

echo "Engram is installed in $PREFIX"
case ":$PATH:" in
  *":$BIN:"*) ;;
  *) echo "Add $BIN to your PATH to run engram and engram-mcp from a terminal." ;;
esac
