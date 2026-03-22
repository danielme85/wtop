#!/bin/sh
set -e

BINARY="wtop"
DEFAULT_INSTALL_DIR="$HOME/.local/bin"

# Use first argument as install dir override, or prompt, or use default
if [ -n "$1" ]; then
    install_dir="$1"
elif [ -t 0 ]; then
    printf "Install directory [%s]: " "$DEFAULT_INSTALL_DIR"
    read -r input
    install_dir="${input:-$DEFAULT_INSTALL_DIR}"
else
    install_dir="$DEFAULT_INSTALL_DIR"
fi

# Check for cargo
if ! command -v cargo >/dev/null 2>&1; then
    printf "Error: cargo is required but not installed.\n" >&2
    printf "Install Rust: https://rustup.rs\n" >&2
    exit 1
fi

# Build release binary
printf "Building %s (release)...\n" "$BINARY"
cargo build --release

# Install
mkdir -p "$install_dir"
cp target/release/"$BINARY" "$install_dir/$BINARY"
chmod +x "$install_dir/$BINARY"

printf "Installed %s to %s/%s\n" "$BINARY" "$install_dir" "$BINARY"

# Check if install dir is in PATH
case ":$PATH:" in
    *":$install_dir:"*) ;;
    *)
        printf "\nNote: %s is not in your PATH.\n" "$install_dir"
        printf "Add it with: export PATH=\"%s:\$PATH\"\n" "$install_dir"
        ;;
esac

printf "Done! Run '%s' to start.\n" "$BINARY"
