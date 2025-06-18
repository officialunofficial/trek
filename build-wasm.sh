#!/bin/bash

# Build Trek for WebAssembly

set -e

echo "Building Trek for WebAssembly..."

# Install wasm-pack if not already installed
if ! command -v wasm-pack &> /dev/null; then
    echo "Installing wasm-pack..."
    curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
fi

# Clean previous builds
rm -rf pkg

# Remove the .cargo/config.toml temporarily to avoid conflicts
if [ -f ".cargo/config.toml" ]; then
    mv .cargo/config.toml .cargo/config.toml.bak
fi

# Build for web target with proper WASM configuration
echo "Building WASM module..."
RUSTFLAGS='--cfg getrandom_backend="js"' wasm-pack build --target web --out-dir pkg --release --no-opt

# Restore config if it was backed up
if [ -f ".cargo/config.toml.bak" ]; then
    mv .cargo/config.toml.bak .cargo/config.toml
fi

# Skip wasm-opt optimization (disabled due to bulk memory operations issue)
echo "Skipping wasm-opt optimization (disabled in Cargo.toml)"

echo "Build complete! Output in pkg/"