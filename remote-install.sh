#!/bin/sh
set -e

REPO="danielme85/wtop"
BINARY="wtop"

# Detect OS
detect_os() {
    os=$(uname -s)
    case "$os" in
        Linux)   echo "linux" ;;
        Darwin)  echo "darwin" ;;
        *)
            printf "Unsupported OS: %s\n" "$os" >&2
            exit 1
            ;;
    esac
}

# Detect architecture
detect_arch() {
    arch=$(uname -m)
    case "$arch" in
        x86_64|amd64)   echo "amd64" ;;
        aarch64|arm64)   echo "arm64" ;;
        *)
            printf "Unsupported architecture: %s\n" "$arch" >&2
            exit 1
            ;;
    esac
}

# Find install directory
find_install_dir() {
    if [ -n "$INSTALL_DIR" ]; then
        echo "$INSTALL_DIR"
        return
    fi

    # Prefer ~/.local/bin (XDG standard), fall back to ~/bin
    for dir in "$HOME/.local/bin" "$HOME/bin"; do
        if [ -d "$dir" ]; then
            echo "$dir"
            return
        fi
    done

    # Default to ~/.local/bin, create it
    echo "$HOME/.local/bin"
}

# Get download URL for the latest release asset
get_download_url() {
    asset_name=$1

    # GitHub Releases API is public — no token needed
    releases_url="https://api.github.com/repos/${REPO}/releases/latest"
    download_url=$(curl -fsSL "$releases_url" \
        | grep -o "\"browser_download_url\": *\"[^\"]*${asset_name}\"" \
        | head -1 | cut -d'"' -f4)

    if [ -z "$download_url" ]; then
        printf "Error: No release asset found for %s.\n" "$asset_name" >&2
        printf "Check https://github.com/%s/releases for available downloads.\n" "$REPO" >&2
        exit 1
    fi

    echo "$download_url"
}

main() {
    os=$(detect_os)
    arch=$(detect_arch)
    install_dir=$(find_install_dir)
    asset_name="${BINARY}-${os}-${arch}"

    printf "Detected platform: %s/%s\n" "$os" "$arch"
    printf "Install directory: %s\n" "$install_dir"

    # Create install dir if needed
    mkdir -p "$install_dir"

    # Download from GitHub Releases (no auth required for public repos)
    printf "Downloading %s...\n" "$asset_name"
    url=$(get_download_url "$asset_name")

    tmpdir=$(mktemp -d)
    trap 'rm -rf "$tmpdir"' EXIT

    curl -fsSL -o "$tmpdir/$asset_name" "$url"

    # Install
    chmod +x "$tmpdir/$asset_name"
    mv "$tmpdir/$asset_name" "$install_dir/$BINARY"

    printf "Installed %s to %s/%s\n" "$asset_name" "$install_dir" "$BINARY"

    # Check if install dir is in PATH
    case ":$PATH:" in
        *":$install_dir:"*) ;;
        *)
            printf "\nNote: %s is not in your PATH.\n" "$install_dir"
            printf "Add it with: export PATH=\"%s:\$PATH\"\n" "$install_dir"
            ;;
    esac

    printf "Done! Run '%s' to start.\n" "$BINARY"
}

main "$@"
