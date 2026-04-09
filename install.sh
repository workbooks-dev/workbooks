#!/bin/sh
# Workbooks CLI installer
# Usage: curl -fsSL https://get.workbooks.dev | sh
set -e

REPO="workbooks-dev/workbooks"
BINARY="wb"
# Install to ~/.local/bin by default (no sudo required).
# Override with WB_INSTALL_DIR=/usr/local/bin sh install.sh
INSTALL_DIR="${WB_INSTALL_DIR:-$HOME/.local/bin}"

# Colors (if terminal supports them)
if [ -t 1 ]; then
    BOLD="\033[1m"
    DIM="\033[2m"
    RESET="\033[0m"
    GREEN="\033[32m"
    RED="\033[31m"
else
    BOLD=""
    DIM=""
    RESET=""
    GREEN=""
    RED=""
fi

info() {
    printf "${BOLD}${GREEN}>${RESET} %s\n" "$1"
}

error() {
    printf "${BOLD}${RED}error:${RESET} %s\n" "$1" >&2
    exit 1
}

# Detect OS
detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "macos" ;;
        *)       error "Unsupported OS: $(uname -s)" ;;
    esac
}

# Detect architecture
detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)  echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *)             error "Unsupported architecture: $(uname -m)" ;;
    esac
}

# Get latest release tag from GitHub
get_latest_version() {
    if command -v curl > /dev/null 2>&1; then
        curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null \
            | grep '"tag_name"' | head -1 | cut -d'"' -f4
    elif command -v wget > /dev/null 2>&1; then
        wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null \
            | grep '"tag_name"' | head -1 | cut -d'"' -f4
    else
        error "Neither curl nor wget found. Please install one."
    fi
}

# Download and install
download() {
    url="$1"
    dest="$2"
    if command -v curl > /dev/null 2>&1; then
        curl -fsSL "$url" -o "$dest"
    elif command -v wget > /dev/null 2>&1; then
        wget -q "$url" -O "$dest"
    fi
}

main() {
    printf "\n"
    info "Installing Workbooks CLI (wb)"

    OS=$(detect_os)
    ARCH=$(detect_arch)

    info "Detected: ${OS}/${ARCH}"

    # Get latest version
    VERSION=$(get_latest_version)
    if [ -z "$VERSION" ]; then
        # Fallback: try to build from source if cargo is available
        if command -v cargo > /dev/null 2>&1; then
            info "No releases found. Building from source..."
            TMPDIR=$(mktemp -d)
            trap "rm -rf $TMPDIR" EXIT

            if command -v git > /dev/null 2>&1; then
                git clone --depth 1 "https://github.com/${REPO}.git" "$TMPDIR/workbooks" 2>/dev/null
                cd "$TMPDIR/workbooks"
                cargo build --release 2>&1 | tail -1

                BUILT_BINARY="$TMPDIR/workbooks/target/release/wb"
                if [ -f "$BUILT_BINARY" ]; then
                    install_binary "$BUILT_BINARY"
                    success
                    return
                fi
            fi
        fi
        error "Could not determine latest version. Check https://github.com/${REPO}/releases"
    fi

    info "Latest version: ${VERSION}"

    # Construct download URL
    ASSET_NAME="wb-${OS}-${ARCH}"
    DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET_NAME}"

    # Download to temp file
    TMPFILE=$(mktemp)
    trap "rm -f $TMPFILE" EXIT

    info "Downloading ${DOWNLOAD_URL}"
    download "$DOWNLOAD_URL" "$TMPFILE" || error "Download failed"

    install_binary "$TMPFILE"
    success
}

install_binary() {
    SRC="$1"
    chmod +x "$SRC"

    # Create target directory if it doesn't exist
    mkdir -p "$INSTALL_DIR" 2>/dev/null || error "Cannot create ${INSTALL_DIR}. Set WB_INSTALL_DIR to a writable directory."

    if [ ! -w "$INSTALL_DIR" ]; then
        error "${INSTALL_DIR} is not writable by current user. Set WB_INSTALL_DIR to a writable directory (e.g. WB_INSTALL_DIR=\$HOME/.local/bin)."
    fi

    mv "$SRC" "${INSTALL_DIR}/${BINARY}"
    chmod +x "${INSTALL_DIR}/${BINARY}"
    info "Installed to ${INSTALL_DIR}/${BINARY}"

    # Check if install dir is in PATH
    case ":$PATH:" in
        *":${INSTALL_DIR}:"*) ;;
        *)
            printf "\n"
            info "${INSTALL_DIR} is not in your PATH. Add this to your shell profile:"
            printf "  export PATH=\"%s:\$PATH\"\n" "${INSTALL_DIR}"
            printf "\n"
            ;;
    esac
}

success() {
    printf "\n"
    info "Workbooks CLI installed!"
    printf "\n"
    printf "  ${DIM}Run a workbook:${RESET}  wb run notebook.md\n"
    printf "  ${DIM}With output:${RESET}     wb run notebook.md -o results.md\n"
    printf "  ${DIM}With secrets:${RESET}    wb run notebook.md --secrets doppler\n"
    printf "  ${DIM}Inspect:${RESET}         wb inspect notebook.md\n"
    printf "\n"
}

main
