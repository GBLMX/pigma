#!/bin/sh
# boxpigma installer for Linux and macOS.
#
# It works out which release asset this machine needs — OS and CPU come from `uname`, the
# C library from the loader — downloads it, checks it against the release's `SHA256SUMS`, and
# drops the binary in place. Nothing else is installed and nothing is written outside the
# target directory.
#
#   curl -fsSL https://raw.githubusercontent.com/GBLMX/pigma/main/install.sh | sh
#   sh install.sh --dir ~/bin --version v1.2.0
#
# Anything the script would guess can be overridden by a flag or an environment variable:
#
#   --version <tag|latest>   BOXPIGMA_VERSION
#   --dir <path>             BOXPIGMA_INSTALL_DIR   (default: ~/.local/bin)
#   --checksums <url|file>   BOXPIGMA_CHECKSUMS     (default: beside the asset)
#   --repo <owner/name>      BOXPIGMA_REPO          (default: GBLMX/pigma)
#   --dry-run                print the plan and stop
#   --force                  install even if the target exists and is the same version

set -eu

REPO="${BOXPIGMA_REPO:-GBLMX/pigma}"
# Where the releases are fetched from. `https://github.com` is the normal answer; a mirror
# or a proxy (for networks that cannot reach GitHub directly) is why this is a knob.
HOST="${BOXPIGMA_GITHUB:-https://github.com}"
VERSION="${BOXPIGMA_VERSION:-latest}"
INSTALL_DIR="${BOXPIGMA_INSTALL_DIR:-}"
CHECKSUMS="${BOXPIGMA_CHECKSUMS:-}"
DRY_RUN=0
FORCE=0

log() { printf '%s\n' "$*" >&2; }
die() { log "install.sh: $*"; exit 1; }

usage() {
    sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
    exit 0
}

while [ $# -gt 0 ]; do
    case "$1" in
        --version) VERSION="${2:?--version needs a value}"; shift 2 ;;
        --dir) INSTALL_DIR="${2:?--dir needs a value}"; shift 2 ;;
        --checksums) CHECKSUMS="${2:?--checksums needs a value}"; shift 2 ;;
        --repo) REPO="${2:?--repo needs a value}"; shift 2 ;;
        --host) HOST="${2:?--host needs a value}"; shift 2 ;;
        --dry-run) DRY_RUN=1; shift ;;
        --force) FORCE=1; shift ;;
        -h | --help) usage ;;
        *) die "unknown argument: $1 (try --help)" ;;
    esac
done

# ---------------------------------------------------------------- platform detection ----

detect_target() {
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$arch" in
        x86_64 | amd64) cpu="x86_64" ;;
        arm64 | aarch64) cpu="aarch64" ;;
        *) die "unsupported CPU: $arch — this project ships x86_64 and aarch64 builds" ;;
    esac

    case "$os" in
        Linux)
            # The musl toolchain is a different target with a different C library; a glibc
            # binary on musl exists but needs a compatibility layer, so say so rather than
            # hand over a binary that may not start.
            if ldd --version 2>&1 | grep -qi musl || [ -e /lib/ld-musl-x86_64.so.1 ] ||
                [ -e /lib/ld-musl-aarch64.so.1 ]; then
                die "musl libc detected — only gnu builds are published; build from source: cargo install --git https://github.com/$REPO.git"
            fi
            target="$cpu-unknown-linux-gnu"
            ;;
        Darwin) target="$cpu-apple-darwin" ;;
        *) die "unsupported OS: $os — on Windows use install.ps1" ;;
    esac

    printf '%s' "$target"
}

TARGET="$(detect_target)"

# The Linux aarch64 asset is built by `cross`, the rest natively; the archives are tar.gz on
# both platforms and contain a single file called `boxpigma`.
case "$TARGET" in
    *-linux-gnu | *-apple-darwin) ASSET="boxpigma-$TARGET.tar.gz" ;;
    *) die "no asset for $TARGET" ;;
esac

if [ -z "$INSTALL_DIR" ]; then
    INSTALL_DIR="$HOME/.local/bin"
fi

if [ "$VERSION" = "latest" ]; then
    BASE="$HOST/$REPO/releases/latest/download"
else
    BASE="$HOST/$REPO/releases/download/$VERSION"
fi
ASSET_URL="$BASE/$ASSET"
[ -n "$CHECKSUMS" ] || CHECKSUMS="$BASE/SHA256SUMS"

log "install.sh: $REPO $VERSION"
log "  platform   $(uname -s) $(uname -m) -> $TARGET"
log "  asset      $ASSET_URL"
log "  install to $INSTALL_DIR/boxpigma"
log "  verify     $CHECKSUMS"

if [ "$DRY_RUN" = 1 ]; then
    log "  (dry run: nothing downloaded or written)"
    exit 0
fi

# ------------------------------------------------------------------------- download ----

fetch() {
    # `fetch <url> <dest>`; curl first, wget as the fallback some minimal images still have.
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$2" "$1"
    else
        die "needs curl or wget"
    fi
}

fetch_optional() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$1" -o "$2" 2>/dev/null
    else
        wget -qO "$2" "$1" 2>/dev/null
    fi
}

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT TERM

log "  downloading…"
fetch "$ASSET_URL" "$TMP/$ASSET"

# ------------------------------------------------------------------------- verify ----

expected_version() {
    # `--version v1.2.0` -> 1.2.0, for the "already installed" check below.
    printf '%s' "$VERSION" | sed 's/^v//'
}

if fetch_optional "$CHECKSUMS" "$TMP/SHA256SUMS"; then
    want="$(awk -v name="$ASSET" '$2 == name { print $1 }' "$TMP/SHA256SUMS")"
    if [ -z "$want" ]; then
        die "SHA256SUMS has no entry for $ASSET"
    fi
    if command -v sha256sum >/dev/null 2>&1; then
        got="$(sha256sum "$TMP/$ASSET" | cut -d' ' -f1)"
    else
        got="$(shasum -a 256 "$TMP/$ASSET" | cut -d' ' -f1)"
    fi
    [ "$want" = "$got" ] || die "checksum mismatch for $ASSET (expected $want, got $got)"
    log "  checksum   ok ($got)"
else
    # A release published before SHA256SUMS existed still installs, but the user is told
    # that this download was not verified rather than left to assume it was.
    log "  checksum   unavailable ($CHECKSUMS) — installing unverified"
fi

# ------------------------------------------------------------------------ install ----

tar -xzf "$TMP/$ASSET" -C "$TMP"
[ -f "$TMP/boxpigma" ] || die "the archive did not contain a 'boxpigma' binary"
chmod +x "$TMP/boxpigma"

if [ -x "$INSTALL_DIR/boxpigma" ] && [ "$FORCE" != 1 ] && [ "$VERSION" != "latest" ]; then
    if "$INSTALL_DIR/boxpigma" --version 2>/dev/null | grep -q "$(expected_version)"; then
        log "  already    $INSTALL_DIR/boxpigma is already $(expected_version) — nothing to do (use --force to reinstall)"
        exit 0
    fi
fi

mkdir -p "$INSTALL_DIR"
# Copy to a temporary name and rename, so a running instance keeps its own file: replacing a
# binary that is executing is allowed on Unix but leaves the running copy unlinked.
cp "$TMP/boxpigma" "$INSTALL_DIR/.boxpigma.new"
chmod 755 "$INSTALL_DIR/.boxpigma.new"
mv -f "$INSTALL_DIR/.boxpigma.new" "$INSTALL_DIR/boxpigma"

log "  installed  $INSTALL_DIR/boxpigma"

case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) log "  note       $INSTALL_DIR is not in PATH — add it, e.g.:
               export PATH=\"$INSTALL_DIR:\$PATH\"" ;;
esac
