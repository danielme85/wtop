#!/bin/sh
set -e

REPO="danielme85/wtop"
BINARY="wtop"

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

# Get latest successful workflow run artifacts
get_download_url() {
    platform=$1

    # Get latest successful run on main branch
    run_url="https://api.github.com/repos/${REPO}/actions/runs?branch=main&status=success&per_page=1"
    if [ -n "$GITHUB_TOKEN" ]; then
        auth_header="Authorization: Bearer $GITHUB_TOKEN"
    elif command -v gh >/dev/null 2>&1; then
        GITHUB_TOKEN=$(gh auth token 2>/dev/null || true)
        if [ -n "$GITHUB_TOKEN" ]; then
            auth_header="Authorization: Bearer $GITHUB_TOKEN"
        fi
    fi

    if [ -z "$auth_header" ]; then
        printf "Error: GitHub token required to download workflow artifacts.\n" >&2
        printf "Set GITHUB_TOKEN or authenticate with: gh auth login\n" >&2
        exit 1
    fi

    run_id=$(curl -fsSL -H "$auth_header" "$run_url" | grep -o '"id":[0-9]*' | head -1 | cut -d: -f2)

    if [ -z "$run_id" ]; then
        printf "Error: No successful workflow runs found.\n" >&2
        exit 1
    fi

    # Get artifact download URL
    artifacts_url="https://api.github.com/repos/${REPO}/actions/runs/${run_id}/artifacts"
    artifact_url=$(curl -fsSL -H "$auth_header" "$artifacts_url" \
        | grep -o "\"archive_download_url\":\"[^\"]*${BINARY}-linux-${platform}[^\"]*\"" \
        | head -1 | cut -d'"' -f4)

    if [ -z "$artifact_url" ]; then
        printf "Error: No artifact found for linux-%s in run %s.\n" "$platform" "$run_id" >&2
        exit 1
    fi

    echo "$artifact_url"
}

main() {
    platform=$(detect_arch)
    install_dir=$(find_install_dir)
    artifact_name="${BINARY}-linux-${platform}"

    printf "Detected platform: linux/%s\n" "$platform"
    printf "Install directory: %s\n" "$install_dir"

    # Create install dir if needed
    mkdir -p "$install_dir"

    # Download
    printf "Downloading %s...\n" "$artifact_name"
    url=$(get_download_url "$platform")

    tmpdir=$(mktemp -d)
    trap 'rm -rf "$tmpdir"' EXIT

    if [ -n "$GITHUB_TOKEN" ]; then
        auth_header="Authorization: Bearer $GITHUB_TOKEN"
    elif command -v gh >/dev/null 2>&1; then
        GITHUB_TOKEN=$(gh auth token 2>/dev/null || true)
        auth_header="Authorization: Bearer $GITHUB_TOKEN"
    fi

    curl -fsSL -H "$auth_header" -o "$tmpdir/artifact.zip" "$url"

    # GitHub artifacts are always zipped
    if command -v unzip >/dev/null 2>&1; then
        unzip -o "$tmpdir/artifact.zip" -d "$tmpdir"
    else
        printf "Error: unzip is required but not installed.\n" >&2
        exit 1
    fi

    # Install
    chmod +x "$tmpdir/$artifact_name"
    mv "$tmpdir/$artifact_name" "$install_dir/$BINARY"

    printf "Installed %s to %s/%s\n" "$artifact_name" "$install_dir" "$BINARY"

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
