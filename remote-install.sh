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

# Print the download URL for a named asset from the release JSON on stdin.
asset_url() {
    grep -o "\"browser_download_url\": *\"https://github.com/${REPO}/releases/download/[^\"]*/$1\"" \
        | head -1 | cut -d'"' -f4
}

# Print the SHA-256 of a file using whichever tool this system has.
sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    else
        printf "Error: need sha256sum or shasum to verify the download.\n" >&2
        exit 1
    fi
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

    tmpdir=$(mktemp -d)
    trap 'rm -rf "$tmpdir"' EXIT

    # Release metadata (GitHub Releases API is public — no token needed)
    curl -fsSL --proto '=https' "https://api.github.com/repos/${REPO}/releases/latest" \
        > "$tmpdir/release.json"
    url=$(asset_url "$asset_name" < "$tmpdir/release.json")
    sums_url=$(asset_url "SHA256SUMS" < "$tmpdir/release.json")

    if [ -z "$url" ]; then
        printf "Error: No release asset found for %s.\n" "$asset_name" >&2
        printf "Check https://github.com/%s/releases for available downloads.\n" "$REPO" >&2
        exit 1
    fi
    if [ -z "$sums_url" ]; then
        printf "Error: the latest release does not publish SHA256SUMS; refusing to install\n" >&2
        printf "an unverified binary. Download manually from https://github.com/%s/releases\n" "$REPO" >&2
        exit 1
    fi

    printf "Downloading %s...\n" "$asset_name"
    curl -fsSL --proto '=https' -o "$tmpdir/$asset_name" "$url"
    curl -fsSL --proto '=https' -o "$tmpdir/SHA256SUMS" "$sums_url"

    # Verify the checksum before installing anything
    expected=$(awk -v f="$asset_name" '$2 == f || $2 == "*"f { print $1; exit }' "$tmpdir/SHA256SUMS")
    actual=$(sha256_of "$tmpdir/$asset_name")
    if [ -z "$expected" ] || [ "$expected" != "$actual" ]; then
        printf "Error: checksum verification failed for %s.\n" "$asset_name" >&2
        printf "  expected: %s\n  actual:   %s\n" "${expected:-<missing>}" "$actual" >&2
        exit 1
    fi
    printf "Verified SHA-256 %s\n" "$actual"

    # Install (copy into the target dir first, then rename, so it is atomic)
    chmod +x "$tmpdir/$asset_name"
    cp "$tmpdir/$asset_name" "$install_dir/.$BINARY.new"
    mv "$install_dir/.$BINARY.new" "$install_dir/$BINARY"

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
