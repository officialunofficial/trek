# Trek Makefile
# Handles building, testing, formatting, and WASM compilation

# Default target
.DEFAULT_GOAL := help

# Variables
CARGO := cargo
WASM_PACK := wasm-pack
PYTHON := python3
RUSTFLAGS_WASM := --cfg getrandom_backend="js"

# Colors for output
RED := \033[0;31m
GREEN := \033[0;32m
YELLOW := \033[0;33m
BLUE := \033[0;34m
NC := \033[0m # No Color

.PHONY: help
help: ## Show this help message
	@echo "$(BLUE)Trek Build System$(NC)"
	@echo "=================="
	@echo ""
	@echo "Available targets:"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  $(GREEN)%-15s$(NC) %s\n", $$1, $$2}'

.PHONY: check
check: ## Check if the code compiles
	@echo "$(YELLOW)Checking code...$(NC)"
	$(CARGO) check

.PHONY: build
build: ## Build the project
	@echo "$(YELLOW)Building project...$(NC)"
	$(CARGO) build

.PHONY: build-release
build-release: ## Build the project in release mode
	@echo "$(YELLOW)Building release...$(NC)"
	$(CARGO) build --release

.PHONY: test
test: ## Run tests
	@echo "$(YELLOW)Running tests...$(NC)"
	$(CARGO) test

.PHONY: test-verbose
test-verbose: ## Run tests with verbose output
	@echo "$(YELLOW)Running tests (verbose)...$(NC)"
	$(CARGO) test -- --nocapture

.PHONY: fmt
fmt: ## Format the code
	@echo "$(YELLOW)Formatting code...$(NC)"
	$(CARGO) fmt

.PHONY: fmt-check
fmt-check: ## Check if code is formatted
	@echo "$(YELLOW)Checking formatting...$(NC)"
	$(CARGO) fmt -- --check

.PHONY: clippy
clippy: ## Run clippy linter
	@echo "$(YELLOW)Running clippy...$(NC)"
	$(CARGO) clippy -- -D warnings

.PHONY: clippy-fix
clippy-fix: ## Run clippy and apply fixes
	@echo "$(YELLOW)Running clippy with fixes...$(NC)"
	$(CARGO) clippy --fix --allow-dirty --allow-staged

.PHONY: clean
clean: ## Clean build artifacts
	@echo "$(YELLOW)Cleaning build artifacts...$(NC)"
	$(CARGO) clean
	rm -rf pkg/
	rm -rf target/

.PHONY: doc
doc: ## Generate documentation
	@echo "$(YELLOW)Generating documentation...$(NC)"
	$(CARGO) doc --open

.PHONY: wasm-check-deps
wasm-check-deps: ## Check WASM dependencies
	@echo "$(YELLOW)Checking WASM dependencies...$(NC)"
	@command -v wasm-pack >/dev/null 2>&1 || { echo "$(RED)wasm-pack not found. Install with: curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh$(NC)" >&2; exit 1; }

.PHONY: wasm-build
wasm-build: wasm-check-deps ## Build WASM module
	@echo "$(YELLOW)Building WASM module...$(NC)"
	RUSTFLAGS='$(RUSTFLAGS_WASM)' $(WASM_PACK) build --target web --out-dir pkg --release --no-opt
	@node scripts/fix-package-json.js

.PHONY: wasm-build-debug
wasm-build-debug: wasm-check-deps ## Build WASM module in debug mode
	@echo "$(YELLOW)Building WASM module (debug)...$(NC)"
	RUSTFLAGS='$(RUSTFLAGS_WASM)' $(WASM_PACK) build --target web --out-dir pkg --dev
	@node scripts/fix-package-json.js

.PHONY: wasm-test
wasm-test: ## Run WASM tests
	@echo "$(YELLOW)Running WASM tests...$(NC)"
	wasm-pack test --headless --chrome

.PHONY: serve
serve: ## Serve the WASM test page
	@echo "$(YELLOW)Starting test server...$(NC)"
	@echo "$(GREEN)Open http://localhost:8000/test-wasm.html in your browser$(NC)"
	$(PYTHON) serve.py

.PHONY: playground
playground: wasm-build ## Build WASM and serve the playground
	@echo "$(YELLOW)Building WASM and starting playground server...$(NC)"
	@echo "$(GREEN)Open http://localhost:8000/playground/ in your browser$(NC)"
	$(PYTHON) serve.py

.PHONY: bench
bench: ## Run benchmarks
	@echo "$(YELLOW)Running benchmarks...$(NC)"
	$(CARGO) bench

.PHONY: outdated
outdated: ## Check for outdated dependencies
	@echo "$(YELLOW)Checking for outdated dependencies...$(NC)"
	$(CARGO) outdated

.PHONY: update
update: ## Update dependencies
	@echo "$(YELLOW)Updating dependencies...$(NC)"
	$(CARGO) update

.PHONY: audit
audit: ## Run security audit
	@echo "$(YELLOW)Running security audit...$(NC)"
	$(CARGO) audit

.PHONY: coverage
coverage: ## Generate test coverage report
	@echo "$(YELLOW)Generating coverage report...$(NC)"
	$(CARGO) tarpaulin --out Html

.PHONY: all
all: fmt check clippy test build ## Run all checks and build

.PHONY: ci
ci: fmt-check check clippy test ## Run CI checks

.PHONY: release
release: clean fmt check clippy test build-release wasm-build ## Build release artifacts

.PHONY: changelog
changelog: ## Generate changelog
	@echo "$(YELLOW)Generating changelog...$(NC)"
	@command -v git-cliff >/dev/null 2>&1 || { echo "$(RED)git-cliff not found. Install with: cargo install git-cliff$(NC)" >&2; exit 1; }
	git-cliff -o CHANGELOG.md

.PHONY: changelog-unreleased
changelog-unreleased: ## Show unreleased changes
	@echo "$(YELLOW)Showing unreleased changes...$(NC)"
	@command -v git-cliff >/dev/null 2>&1 || { echo "$(RED)git-cliff not found. Install with: cargo install git-cliff$(NC)" >&2; exit 1; }
	git-cliff --unreleased

.PHONY: changelog-tag
changelog-tag: ## Generate changelog for a specific tag
	@echo "$(YELLOW)Generating changelog for tag $(TAG)...$(NC)"
	@command -v git-cliff >/dev/null 2>&1 || { echo "$(RED)git-cliff not found. Install with: cargo install git-cliff$(NC)" >&2; exit 1; }
	@test -n "$(TAG)" || { echo "$(RED)Please specify TAG=v0.x.x$(NC)" >&2; exit 1; }
	git-cliff --tag $(TAG) -o CHANGELOG.md

.PHONY: version-bump
version-bump: ## Suggest next version based on commits
	@echo "$(YELLOW)Analyzing commits for version bump...$(NC)"
	@command -v git-cliff >/dev/null 2>&1 || { echo "$(RED)git-cliff not found. Install with: cargo install git-cliff$(NC)" >&2; exit 1; }
	@echo "Current version: $$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)"
	@echo "Suggested next version based on commits:"
	@git-cliff --bumped-version

.PHONY: install-dev-deps
install-dev-deps: ## Install development dependencies
	@echo "$(YELLOW)Installing development dependencies...$(NC)"
	$(CARGO) install cargo-outdated
	$(CARGO) install cargo-audit
	$(CARGO) install cargo-tarpaulin
	$(CARGO) install git-cliff
	$(CARGO) install wasm-pack
	@echo "$(GREEN)Development dependencies installed!$(NC)"

.PHONY: setup-git
setup-git: ## Configure git for conventional commits
	@echo "$(YELLOW)Configuring git for conventional commits...$(NC)"
	git config --local commit.template .gitmessage
	@echo "$(GREEN)Git configured to use .gitmessage template!$(NC)"
	@echo "$(BLUE)Tip: Use 'git commit' (without -m) to use the template$(NC)"

.PHONY: pre-commit
pre-commit: fmt check clippy test ## Run pre-commit checks
	@echo "$(GREEN)All pre-commit checks passed!$(NC)"

.PHONY: npm-publish
npm-publish: wasm-build ## Publish to npm registry
	@echo "$(YELLOW)Publishing to npm...$(NC)"
	@cd pkg && npm publish --access public
	@echo "$(GREEN)Published to npm!$(NC)"

.PHONY: npm-publish-dry
npm-publish-dry: wasm-build ## Dry run npm publish
	@echo "$(YELLOW)Running npm publish dry run...$(NC)"
	@cd pkg && npm publish --dry-run --access public

.PHONY: crates-publish
crates-publish: test ## Publish to crates.io
	@echo "$(YELLOW)Publishing to crates.io...$(NC)"
	cargo publish
	@echo "$(GREEN)Published to crates.io!$(NC)"

.PHONY: crates-publish-dry
crates-publish-dry: test ## Dry run crates.io publish
	@echo "$(YELLOW)Running crates.io publish dry run...$(NC)"
	cargo publish --dry-run

.PHONY: stats
stats: ## Show code statistics
	@echo "$(YELLOW)Code statistics:$(NC)"
	@echo ""
	@echo "Lines of Rust code:"
	@find src -name "*.rs" | xargs wc -l | tail -1
	@echo ""
	@echo "Number of files:"
	@find src -name "*.rs" | wc -l

# Create a simple VS Code tasks.json if it doesn't exist
.PHONY: vscode-setup
vscode-setup: ## Setup VS Code tasks
	@mkdir -p .vscode
	@echo '{\n  "version": "2.0.0",\n  "tasks": [\n    {\n      "label": "cargo check",\n      "type": "shell",\n      "command": "make check"\n    },\n    {\n      "label": "cargo test",\n      "type": "shell",\n      "command": "make test"\n    },\n    {\n      "label": "cargo fmt",\n      "type": "shell",\n      "command": "make fmt"\n    }\n  ]\n}' > .vscode/tasks.json
	@echo "$(GREEN)VS Code tasks created!$(NC)"