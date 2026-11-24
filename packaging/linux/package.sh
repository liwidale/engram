#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:?usage: package.sh <version>}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET="${CARGO_TARGET_DIR:-$ROOT/rust-core/target}/release"
NAME="engram-v$VERSION-linux-$(uname -m)"
STAGE="$ROOT/dist/$NAME"

rm -rf "$STAGE"
mkdir -p "$STAGE/icons"
cp "$TARGET/engram" "$TARGET/engram-mcp" "$STAGE/"
cp "$ROOT/packaging/linux/com.liwidale.engram.desktop" "$ROOT/packaging/linux/com.liwidale.engram.xml" "$STAGE/"
cp "$ROOT/packaging/linux/install.sh" "$STAGE/"
cp "$ROOT"/branding/icons/engram-*.png "$STAGE/icons/"
cp "$ROOT/LICENSE" "$STAGE/"
tar -C "$ROOT/dist" -czf "$ROOT/dist/$NAME.tar.gz" "$NAME"
rm -rf "$STAGE"
echo "$ROOT/dist/$NAME.tar.gz"
