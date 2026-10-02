#!/usr/bin/env bash
# Aoxn one-click installer (Linux + macOS).
#
#   curl -fsSL https://raw.githubusercontent.com/AlonechatWorkspace/Aoxn-language/main/dist/install.sh | bash
#
# Downloads the prebuilt toolchain archive for this platform, unpacks it into
# a self-contained directory, puts `aoxn` on PATH, checks for the C toolchain
# (clang) the compiler shells out to, and finishes with `aoxn doctor`.
#
# Options:
#   --version <v>     install a specific version (default: latest release)
#   --prefix <dir>    install root (default: $AOXN_HOME, else ~/.aoxn)
#   --archive <path>  install from a local .tar.gz / unpacked dir (offline)
#   --from-source     skip the download and `cargo install --path .` the checkout
#                     this script lives in (needs a Rust toolchain)
#   --no-clang        do not try to install the C toolchain
#   --no-path         do not touch PATH (link into $PREFIX/bin only)
#   --force           overwrite an existing install without asking
#   --uninstall       remove the install root and the PATH entry
set -euo pipefail

REPO="AlonechatWorkspace/Aoxn-language"
VERSION=""
PREFIX="${AOXN_HOME:-}"
ARCHIVE=""
FROM_SOURCE=0
INSTALL_CLANG=1
TOUCH_PATH=1
FORCE=0
UNINSTALL=0
BIN_DIR="${HOME}/.local/bin"

say()  { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --version)    VERSION="${2:?--version needs a value}"; shift 2 ;;
    --prefix)     PREFIX="${2:?--prefix needs a value}"; shift 2 ;;
    --archive)    ARCHIVE="${2:?--archive needs a value}"; shift 2 ;;
    --from-source) FROM_SOURCE=1; shift ;;
    --no-clang)   INSTALL_CLANG=0; shift ;;
    --no-path)    TOUCH_PATH=0; shift ;;
    --force|-f)   FORCE=1; shift ;;
    --uninstall)  UNINSTALL=1; shift ;;
    -h|--help)    sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *)            die "unknown option '$1' (try --help)" ;;
  esac
done

# ---- platform detection -----------------------------------------------------
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
case "$OS" in
  linux) OS_TAG="linux" ;;
  darwin) OS_TAG="macos" ;;
  *) die "unsupported OS '$OS' — this installer covers Linux and macOS" ;;
esac
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64|amd64) ARCH_TAG="x86_64" ;;
  arm64|aarch64) ARCH_TAG="arm64" ;;
  *) die "unsupported architecture '$ARCH'" ;;
esac
PLATFORM="${OS_TAG}-${ARCH_TAG}"

[ -n "$PREFIX" ] || PREFIX="$HOME/.aoxn"
PREFIX="${PREFIX%/}"
AOXN_BIN="$PREFIX/bin/aoxn"

# ---- uninstall --------------------------------------------------------------
if [ "$UNINSTALL" = 1 ]; then
  say "removing $PREFIX"
  command rm -rf "$PREFIX"
  if [ -L "$BIN_DIR/aoxn" ]; then command rm -f "$BIN_DIR/aoxn"; fi
  say "done — PATH still lists these; edit your shell profile if needed:"
  echo "    $BIN_DIR"
  exit 0
fi

# ---- refuse to clobber ------------------------------------------------------
if [ -e "$AOXN_BIN" ] && [ "$FORCE" != 1 ]; then
  say "Aoxn is already installed at $PREFIX — upgrading"
  CURRENT="$("$AOXN_BIN" version 2>/dev/null | awk '{print $2}' || true)"
  [ "$CURRENT" = "$VERSION" ] && [ -n "$VERSION" ] && say "already at v$CURRENT"
fi

TMP="$(mktemp -d)"
cleanup() { command rm -rf "$TMP"; }
trap cleanup EXIT

# ---- obtain the toolchain ---------------------------------------------------
if [ "$FROM_SOURCE" = 1 ]; then
  SRC_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
  [ -f "$SRC_ROOT/Cargo.toml" ] || die "--from-source must run inside the Aoxn checkout"
  say "building from source in $SRC_ROOT (cargo install --path .)"
  command -v cargo >/dev/null 2>&1 || die "cargo not found; install Rust from https://rustup.rs"
  (cd "$SRC_ROOT" && cargo install --path . --locked --force)
  say "installed the cargo bin; copying the stdlib next to it"
  BIN_DIR_REAL="$(dirname "$(command -v cargo)")"
  say "cargo installs to $BIN_DIR_REAL/aoxn — use that one directly"
  "$BIN_DIR_REAL/aoxn" doctor || true
  exit 0
fi

mkdir -p "$PREFIX/lib/stdlib"

if [ -n "$ARCHIVE" ]; then
  [ -e "$ARCHIVE" ] || die "archive not found: $ARCHIVE"
  say "installing from local archive $ARCHIVE"
  SRC="$ARCHIVE"
else
  if [ -z "$VERSION" ]; then
    say "looking up the latest Aoxn release"
    JSON_URL="https://api.github.com/repos/$REPO/releases/latest"
    if command -v curl >/dev/null 2>&1; then
      VERSION="$(curl -fsSL "$JSON_URL" | sed -n 's/.*"tag_name" *: *"v\([^"]*\)".*/\1/p' | head -1)"
    elif command -v wget >/dev/null 2>&1; then
      VERSION="$(wget -qO- "$JSON_URL" | sed -n 's/.*"tag_name" *: *"v\([^"]*\)".*/\1/p' | head -1)"
    fi
    [ -n "$VERSION" ] || die "could not reach GitHub; pass --version <v> or --archive <path>"
  fi
  ASSET="aoxn-v${VERSION}-${PLATFORM}.tar.gz"
  URL="https://github.com/$REPO/releases/download/v${VERSION}/${ASSET}"
  say "downloading $ASSET"
  SRC="$TMP/$ASSET"
  if command -v curl >/dev/null 2>&1; then
    curl -fL --progress-bar -o "$SRC" "$URL" || die "download failed: $URL"
  elif command -v wget >/dev/null 2>&1; then
    wget -q --show-progress -O "$SRC" "$URL" || die "download failed: $URL"
  else
    die "neither curl nor wget is available"
  fi
fi

# ---- unpack -----------------------------------------------------------------
STAGE="$TMP/stage"
mkdir -p "$STAGE"
say "unpacking"
if [ -d "$SRC" ]; then
  cp -R "$SRC/." "$STAGE/"
else
  tar -xzf "$SRC" -C "$STAGE"
fi
[ -x "$STAGE/bin/aoxn" ] || die "archive does not contain bin/aoxn (wrong platform? $PLATFORM)"

say "installing to $PREFIX"
mkdir -p "$PREFIX/bin" "$PREFIX/lib"
command rm -rf "$PREFIX/lib/stdlib"
command cp -R "$STAGE/." "$PREFIX/"
chmod +x "$PREFIX/bin/aoxn"

# ---- PATH -------------------------------------------------------------------
if [ "$TOUCH_PATH" = 1 ]; then
  mkdir -p "$BIN_DIR"
  ln -sf "$AOXN_BIN" "$BIN_DIR/aoxn"
  case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *)
      say "add this to your shell profile to use 'aoxn' by name:"
      echo "    export PATH=\"$BIN_DIR:\$PATH\""
      ;;
  esac
fi

# ---- C toolchain ------------------------------------------------------------
ensure_clang() {
  if command -v clang >/dev/null 2>&1; then
    say "clang found: $(command -v clang)"
    return 0
  fi
  if [ "$INSTALL_CLANG" = 0 ]; then
    warn "no clang found and --no-clang was given"
    warn "install one (apt-get install clang / brew install llvm) — 'aoxn run' needs it"
    return 1
  fi
  say "no clang found — the C backend needs one; trying the platform package manager"
  if [ "$OS_TAG" = "macos" ]; then
    if command -v brew >/dev/null 2>&1; then
      brew install llvm || true
    elif command -v xcode-select >/dev/null 2>&1; then
      warn "installing Xcode command line tools (may prompt)"
      xcode-select --install || true
    fi
  else
    if command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
      if command -v apt-get >/dev/null 2>&1; then sudo apt-get update && sudo apt-get install -y clang || true
      elif command -v dnf >/dev/null 2>&1; then sudo dnf install -y clang || true
      elif command -v pacman >/dev/null 2>&1; then sudo pacman -S --noconfirm clang || true
      fi
    fi
  fi
  if command -v clang >/dev/null 2>&1; then
    say "clang installed: $(command -v clang)"
    return 0
  fi
  warn "could not install clang automatically"
  warn "install it with: apt-get install clang | dnf install clang | pacman -S clang | brew install llvm"
  return 1
}

CLANG_OK=0
ensure_clang || CLANG_OK=$?

# ---- verify -----------------------------------------------------------------
say "running 'aoxn doctor'"
set +e
"$AOXN_BIN" doctor
DOCTOR_STATUS=$?
set -e

echo
say "Aoxn installed: $AOXN_BIN"
"$AOXN_BIN" version || true
echo
say "try it:"
echo "    $AOXN_BIN run $PREFIX/examples/hello.ax"
echo "    (or 'aoxn run ...' once $BIN_DIR is on your PATH)"
exit "$DOCTOR_STATUS"