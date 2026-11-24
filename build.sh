#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: ./build.sh [--debug] [--native] [--core-only] [--test] [--install]

Builds libengram_core.a and its C header, engram-mcp and Engram.app (macOS).

  --debug      build unoptimized
  --native     build only for the host architecture
  --core-only  stop after the Rust artifacts
  --test       run the Rust and Swift test suites
  --install    copy Engram.app to /Applications and register it with Launch Services
USAGE
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUST="$ROOT/rust-core"
APP_PKG="$ROOT/EngramApp"
VENDOR="$APP_PKG/Vendor"
OUT="$ROOT/build"
VERSION="0.1.0"

PROFILE=release; NATIVE=0; CORE_ONLY=0; RUN_TESTS=0; INSTALL=0
for arg in "$@"; do
  case "$arg" in
    --debug) PROFILE=debug ;;
    --native) NATIVE=1 ;;
    --core-only) CORE_ONLY=1 ;;
    --test) RUN_TESTS=1 ;;
    --install) INSTALL=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "error: unknown option $arg — run ./build.sh --help to see the options" >&2; exit 2 ;;
  esac
done

log() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
need() { command -v "$1" >/dev/null 2>&1 || { echo "error: $1 is not installed — $2" >&2; exit 1; }; }

need cargo "install Rust from https://rustup.rs"
need swift "install Xcode or the Command Line Tools"
need lipo "install the Command Line Tools"
if ! command -v cbindgen >/dev/null 2>&1; then
  log "Installing cbindgen"
  cargo install cbindgen --locked
fi

if [[ $NATIVE -eq 1 ]]; then
  case "$(uname -m)" in
    arm64) ARCHS=(arm64) ;;
    *) ARCHS=(x86_64) ;;
  esac
else
  ARCHS=(arm64 x86_64)
fi
rust_triple() {
  if [[ $1 == arm64 ]]; then echo aarch64-apple-darwin; else echo x86_64-apple-darwin; fi
}

export MACOSX_DEPLOYMENT_TARGET=14.0
CARGO_FLAGS=(); [[ $PROFILE == release ]] && CARGO_FLAGS+=(--release)

if [[ $RUN_TESTS -eq 1 ]]; then
  log "Running Rust tests"
  (cd "$RUST" && cargo test --workspace)
fi

LIBS=(); BINS=()
for arch in "${ARCHS[@]}"; do
  triple="$(rust_triple "$arch")"
  if command -v rustup >/dev/null 2>&1; then
    rustup target list --installed | grep -qx "$triple" || rustup target add "$triple"
  fi
  log "Building engram-core + engram-mcp ($triple, $PROFILE)"
  (cd "$RUST" && cargo build ${CARGO_FLAGS[@]+"${CARGO_FLAGS[@]}"} --target "$triple" -p engram-core -p engram-mcp)
  LIBS+=("$RUST/target/$triple/$PROFILE/libengram_core.a")
  BINS+=("$RUST/target/$triple/$PROFILE/engram-mcp")
done

mkdir -p "$VENDOR/lib" "$VENDOR/include" "$OUT"
log "Creating universal libengram_core.a (${ARCHS[*]})"
lipo -create "${LIBS[@]}" -output "$VENDOR/lib/libengram_core.a"
log "Creating universal engram-mcp"
lipo -create "${BINS[@]}" -output "$OUT/engram-mcp"

log "Generating C header with cbindgen"
(cd "$RUST" && cbindgen --quiet --config cbindgen.toml --crate engram-core --output "$VENDOR/include/engram_core.h")

if [[ $RUN_TESTS -eq 1 ]]; then
  log "Running Swift bridge tests"
  TEST_FLAGS=()
  CLT_DEV=/Library/Developer/CommandLineTools/Library/Developer
  if [[ "$(xcode-select -p 2>/dev/null)" == /Library/Developer/CommandLineTools* && -d "$CLT_DEV/Frameworks/Testing.framework" ]]; then
    TEST_FLAGS=(-Xswiftc -F -Xswiftc "$CLT_DEV/Frameworks" -Xlinker -F -Xlinker "$CLT_DEV/Frameworks"
                -Xlinker -rpath -Xlinker "$CLT_DEV/Frameworks" -Xlinker -rpath -Xlinker "$CLT_DEV/usr/lib")
  fi
  swift test --package-path "$APP_PKG" ${TEST_FLAGS[@]+"${TEST_FLAGS[@]}"}
fi

if [[ $RUN_TESTS -eq 1 ]]; then
  log "Checking Russian translations"
  STRINGS_DIR="$(mktemp -d)"
  swift build --package-path "$APP_PKG" --product Engram \
    -Xswiftc -emit-localized-strings -Xswiftc -emit-localized-strings-path -Xswiftc "$STRINGS_DIR"
  swift "$ROOT/tools/check-strings.swift" "$STRINGS_DIR" "$APP_PKG/Support/ru.lproj/Localizable.strings"
  rm -rf "$STRINGS_DIR"
fi

if [[ $CORE_ONLY -eq 1 ]]; then
  log "Rust artifacts ready: $VENDOR/lib, $VENDOR/include, $OUT/engram-mcp"
  exit 0
fi

APP_BINS=()
for arch in "${ARCHS[@]}"; do
  log "Building Engram.app executable ($arch, $PROFILE)"
  swift build --package-path "$APP_PKG" -c "$PROFILE" --arch "$arch" --product Engram
  APP_BINS+=("$APP_PKG/.build/$arch-apple-macosx/$PROFILE/Engram")
done

APP="$OUT/Engram.app"
log "Assembling $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create "${APP_BINS[@]}" -output "$APP/Contents/MacOS/Engram"
cp "$OUT/engram-mcp" "$APP/Contents/MacOS/engram-mcp"
BUILD_NUM="$(git -C "$ROOT" rev-list --count HEAD 2>/dev/null || echo 1)"
sed -e "s/__VERSION__/$VERSION/" -e "s/__BUILD__/$BUILD_NUM/" "$APP_PKG/Support/Info.plist" > "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"
if [[ -f "$APP_PKG/Support/AppIcon.icns" ]]; then
  cp "$APP_PKG/Support/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"
fi
cp "$ROOT/branding/logo.png" "$APP/Contents/Resources/Logo.png"
cp -R "$APP_PKG/Support/ru.lproj" "$APP/Contents/Resources/ru.lproj"
mkdir -p "$APP/Contents/Resources/en.lproj"
plutil -lint "$APP/Contents/Info.plist" >/dev/null

log "Signing (ad-hoc)"
codesign --force --sign - --timestamp=none "$APP/Contents/MacOS/engram-mcp"
codesign --force --sign - --timestamp=none "$APP"

if [[ $INSTALL -eq 1 ]]; then
  log "Installing to /Applications/Engram.app"
  rm -rf /Applications/Engram.app
  cp -R "$APP" /Applications/Engram.app
  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f /Applications/Engram.app
fi

log "Done: $APP"
lipo -info "$APP/Contents/MacOS/Engram" "$APP/Contents/MacOS/engram-mcp"
