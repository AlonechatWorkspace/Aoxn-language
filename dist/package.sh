#!/usr/bin/env bash
# Package a built Aoxn compiler into a distributable toolchain archive.
#
#   dist/package.sh <path-to-aoxn-binary> <version> [output-dir]
#
# Produces <out>/aoxn-v<version>-<os>-<arch>.{tar.gz,zip} containing the
# install layout documented in docs/install.md:
#
#   bin/aoxn[.exe]      the compiler driver
#   lib/stdlib/*.ax     stdlib + the UI toolkit
#   examples/*.ax       runnable samples
#   dist/install.*      the one-click installers
#   README LICENSE CHANGELOG docs/install.md VERSION
#
# Run from the repository root. CI (.github/workflows/release.yml) calls this
# on all three platforms; it works under Git Bash on Windows too.
set -euo pipefail

BIN="${1:?usage: package.sh <aoxn-binary> <version> [outdir]}"
VER="${2:?usage: package.sh <aoxn-binary> <version> [outdir]}"
OUT="${3:-dist/release}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# an absolute output path must not be glued onto the repo root below
case "$OUT" in
  /*|[A-Za-z]:*) ;;
  *) OUT="$ROOT/$OUT" ;;
esac
cd "$ROOT"

[ -f "$BIN" ] || { echo "error: binary not found: $BIN" >&2; exit 1; }

case "$(uname -s)" in
  Linux*)  OS_TAG=linux ;;
  Darwin*) OS_TAG=macos ;;
  MINGW*|MSYS*|CYGWIN*) OS_TAG=windows ;;
  *) echo "error: unsupported OS $(uname -s)" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64|amd64) ARCH_TAG=x86_64 ;;
  arm64|aarch64) ARCH_TAG=arm64 ;;
  *) ARCH_TAG="$(uname -m)" ;;
esac
PLATFORM="${OS_TAG}-${ARCH_TAG}"
STAGE="$(mktemp -d)/aoxn"
trap 'rm -rf "$(dirname "$STAGE")"' EXIT

echo "==> staging the toolchain for $PLATFORM"
mkdir -p "$STAGE/bin" "$STAGE/lib/stdlib" "$STAGE/examples" "$STAGE/dist" "$STAGE/docs"
cp "$BIN" "$STAGE/bin/aoxn$([ "$OS_TAG" = windows ] && echo .exe)"
cp stdlib/*.ax "$STAGE/lib/stdlib/"
cp examples/*.ax "$STAGE/examples/"
cp dist/install.sh dist/install.ps1 "$STAGE/dist/"
cp README.md LICENSE CHANGELOG.md "$STAGE/"
cp docs/install.md "$STAGE/docs/"
printf '%s\n' "$VER" > "$STAGE/VERSION"
chmod +x "$STAGE/bin/aoxn" 2>/dev/null || true
chmod +x "$STAGE/dist/install.sh" 2>/dev/null || true

mkdir -p "$OUT"
NAME="aoxn-v${VER}-${PLATFORM}"
if [ "$OS_TAG" = windows ]; then
  echo "==> zipping"
  # Git Bash rewrites POSIX-looking arguments handed to native programs, so
  # convert to a Windows path and keep it away from that rewriting.
  WIN_STAGE="$(cygpath -w "$(dirname "$STAGE")")"
  WIN_ZIP="$(cygpath -w "$OUT/$NAME.zip")"
  MSYS2_ARG_CONV_EXCL='*' powershell -NoProfile -Command \
    "Compress-Archive -Path '$WIN_STAGE\aoxn\*' -DestinationPath '$WIN_ZIP' -Force"
  SHA="$(MSYS2_ARG_CONV_EXCL='*' powershell -NoProfile -Command \
    "(Get-FileHash -Algorithm SHA256 '$WIN_ZIP').Hash.ToLower()" | tr -d '\r')"
else
  echo "==> tarring"
  tar -czf "$OUT/$NAME.tar.gz" -C "$(dirname "$STAGE")" aoxn
  if command -v sha256sum >/dev/null 2>&1; then
    SHA="$(sha256sum "$OUT/$NAME.tar.gz" | cut -d' ' -f1)"
  else
    SHA="$(shasum -a 256 "$OUT/$NAME.tar.gz" | cut -d' ' -f1)"
  fi
fi
echo "$SHA  $NAME" > "$OUT/$NAME.sha256"
echo "==> $OUT/$NAME (sha256 $SHA)"