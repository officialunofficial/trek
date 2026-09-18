# Trek justfile
# Mirrors Makefile one for one. Run `just` or `just --list` to see every
# recipe. Prefer either tool; CI and CONTRIBUTING.md document both.

set shell := ["bash", "-euc"]

cargo := "cargo"
wasm_pack := "wasm-pack"
python := "python3"
rustflags_wasm := '--cfg getrandom_backend="wasm_js"'

# Show every available recipe
default:
    @just --list

# Check if the code compiles
check:
    {{ cargo }} check

# Build the project
build:
    {{ cargo }} build

# Build the project in release mode
build-release:
    {{ cargo }} build --release

# Run tests
test:
    {{ cargo }} test

# Run tests with verbose output
test-verbose:
    {{ cargo }} test -- --nocapture

# Run the metadata-only Defuddle parity fixtures
test-fixtures:
    {{ cargo }} test --test fixtures_test

# Run fixtures parity including markdown-body diff
test-fixtures-full:
    {{ cargo }} test --test fixtures_test --features markdown-fixtures

# Regenerate tests/expected/*.md from current Trek output
update-fixtures:
    TREK_UPDATE_FIXTURES=1 {{ cargo }} test --test fixtures_test --features markdown-fixtures -- --nocapture

# Format the code
fmt:
    {{ cargo }} fmt

# Check if code is formatted
fmt-check:
    {{ cargo }} fmt -- --check

# Run clippy linter
clippy:
    {{ cargo }} clippy -- -D warnings

# Run clippy and apply fixes
clippy-fix:
    {{ cargo }} clippy --fix --allow-dirty --allow-staged

# Clean build artifacts
clean:
    {{ cargo }} clean
    rm -rf pkg/
    rm -rf target/

# Generate documentation
doc:
    {{ cargo }} doc --open

# Check WASM dependencies
wasm-check-deps:
    command -v wasm-pack >/dev/null 2>&1 || { echo "wasm-pack not found. Install with: curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh" >&2; exit 1; }

# Build WASM module
wasm-build: wasm-check-deps
    RUSTFLAGS='{{ rustflags_wasm }}' {{ wasm_pack }} build --target web --out-dir pkg --release --no-opt
    node scripts/fix-package-json.js

# Build WASM module in debug mode
wasm-build-debug: wasm-check-deps
    RUSTFLAGS='{{ rustflags_wasm }}' {{ wasm_pack }} build --target web --out-dir pkg --dev
    node scripts/fix-package-json.js

# Run WASM tests
wasm-test:
    wasm-pack test --headless --chrome

# Serve the WASM test page
serve:
    @echo "Open http://localhost:8000/test-wasm.html in your browser"
    {{ python }} serve.py

# Build WASM and serve the playground
playground: wasm-build
    @echo "Open http://localhost:8000/playground/ in your browser"
    {{ python }} serve.py

# Run benchmarks
bench:
    {{ cargo }} bench

# Check for outdated dependencies
outdated:
    {{ cargo }} outdated

# Update dependencies
update:
    {{ cargo }} update

# Run security audit
audit:
    {{ cargo }} audit

# Check licenses, advisories, bans, and sources with cargo-deny
deny:
    {{ cargo }} deny check

# Generate test coverage report
coverage:
    {{ cargo }} tarpaulin --out Html

# Run all checks and build
all: fmt check clippy test build

# Run CI checks
ci: fmt-check check clippy test

# Build release artifacts
release: clean fmt check clippy test build-release wasm-build

# Generate changelog
changelog:
    command -v git-cliff >/dev/null 2>&1 || { echo "git-cliff not found. Install with: cargo install git-cliff" >&2; exit 1; }
    git-cliff -o CHANGELOG.md

# Show unreleased changes
changelog-unreleased:
    command -v git-cliff >/dev/null 2>&1 || { echo "git-cliff not found. Install with: cargo install git-cliff" >&2; exit 1; }
    git-cliff --unreleased

# Generate changelog for a specific tag, e.g. `just changelog-tag v0.3.0`
changelog-tag tag:
    command -v git-cliff >/dev/null 2>&1 || { echo "git-cliff not found. Install with: cargo install git-cliff" >&2; exit 1; }
    git-cliff --tag {{ tag }} -o CHANGELOG.md

# Suggest next version based on commits
version-bump:
    command -v git-cliff >/dev/null 2>&1 || { echo "git-cliff not found. Install with: cargo install git-cliff" >&2; exit 1; }
    echo "Current version: $(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)"
    echo "Suggested next version based on commits:"
    git-cliff --bumped-version

# Install development dependencies
install-dev-deps:
    {{ cargo }} install cargo-outdated
    {{ cargo }} install cargo-audit
    {{ cargo }} install cargo-deny
    {{ cargo }} install cargo-tarpaulin
    {{ cargo }} install git-cliff
    {{ cargo }} install wasm-pack --version 0.15.0 --locked
    @echo "Development dependencies installed!"

# Configure git for conventional commits
setup-git:
    git config --local commit.template .gitmessage
    @echo "Git configured to use .gitmessage template!"
    @echo "Tip: Use 'git commit' (without -m) to use the template"

# Install git hooks for commit validation
setup-hooks:
    mkdir -p .git/hooks
    echo '#!/bin/bash' > .git/hooks/commit-msg
    echo './scripts/validate-commit.sh "$1"' >> .git/hooks/commit-msg
    chmod +x .git/hooks/commit-msg
    @echo "Git hooks installed!"
    @echo "Commits will now be validated for conventional format"

# Run pre-commit checks
pre-commit: fmt check clippy test
    @echo "All pre-commit checks passed!"

# Run the Trek Cloudflare Worker locally
worker-dev:
    cd worker && npx wrangler dev

# Deploy the Trek Cloudflare Worker
worker-deploy:
    cd worker && npx wrangler deploy

# Dry-run the Trek Worker deploy and emit the bundle
worker-deploy-dry:
    cd worker && npx wrangler deploy --dry-run --outdir=dist

# Publish to npm registry
npm-publish: wasm-build
    cd pkg && npm publish --access public
    @echo "Published to npm!"

# Dry run npm publish
npm-publish-dry: wasm-build
    cd pkg && npm publish --dry-run --access public

# Publish to crates.io
crates-publish: test
    cargo publish
    @echo "Published to crates.io!"

# Dry run crates.io publish
crates-publish-dry: test
    cargo publish --dry-run

# Show code statistics
stats:
    @echo "Lines of Rust code:"
    find src -name "*.rs" | xargs wc -l | tail -1
    @echo ""
    @echo "Number of files:"
    find src -name "*.rs" | wc -l

# Set up VS Code tasks
vscode-setup:
    mkdir -p .vscode
    printf '%s\n' \
        '{' \
        '  "version": "2.0.0",' \
        '  "tasks": [' \
        '    { "label": "cargo check", "type": "shell", "command": "just check" },' \
        '    { "label": "cargo test", "type": "shell", "command": "just test" },' \
        '    { "label": "cargo fmt", "type": "shell", "command": "just fmt" }' \
        '  ]' \
        '}' > .vscode/tasks.json
    @echo "VS Code tasks created!"
