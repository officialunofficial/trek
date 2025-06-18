# Getting Started

This guide will help you set up your development environment for working with Trek.

## Prerequisites

- Rust (stable channel)
- Node.js (for WebAssembly development)
- Python 3 (for development server)
- Make (for build commands)

## Installation

### 1. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. Add WebAssembly Target

```bash
rustup target add wasm32-unknown-unknown
```

### 3. Clone the Repository

```bash
git clone https://github.com/officialunofficial/trek.git
cd trek
```

### 4. Install Development Dependencies

```bash
make install-dev-deps
```

This will install:
- `wasm-pack`: For building WebAssembly packages
- `cargo-tarpaulin`: For code coverage
- `cargo-watch`: For development auto-reload

## Building Trek

### Native Build (Rust)

```bash
# Debug build
make build

# Release build
make build-release
```

### WebAssembly Build

```bash
# Release build (optimized for size)
make wasm-build

# Debug build (with debug symbols)
make wasm-build-debug
```

The WebAssembly build outputs will be in the `pkg/` directory.

## Development Workflow

### 1. Check Your Code

Before making changes, ensure everything compiles:

```bash
make check
```

### 2. Format Code

Trek uses rustfmt for consistent code formatting:

```bash
make fmt
```

### 3. Run Linter

Use clippy for catching common mistakes:

```bash
make clippy
```

### 4. Run Tests

```bash
# Run all tests
make test

# Run tests with output
make test-verbose

# Run WebAssembly tests
make wasm-test

# Run benchmarks
make bench
```

### 5. Pre-commit Checks

Before committing, run all checks:

```bash
make pre-commit
```

This runs:
- Code formatting
- Compilation check
- Linter
- Tests

### 6. Development Server

To test WebAssembly builds in the browser:

```bash
make serve
```

Then open http://localhost:8000/test-wasm.html in your browser.

## Project Structure

```
trek/
├── src/
│   ├── lib.rs              # Main Trek struct and API
│   ├── extractor.rs        # Extractor trait and registry
│   ├── extractors/         # Site-specific extractors
│   │   ├── mod.rs
│   │   └── generic.rs
│   ├── types.rs            # Core types and structures
│   ├── constants.rs        # Configuration constants
│   ├── error.rs            # Error types
│   ├── utils.rs            # Utility functions
│   └── wasm.rs             # WebAssembly bindings
├── tests/                  # Integration tests
├── benches/                # Benchmarks
├── pkg/                    # WebAssembly build output
├── Cargo.toml              # Rust dependencies
├── Makefile                # Build commands
└── test-wasm.html          # WebAssembly test page
```

## Common Development Tasks

### Adding a New Feature

1. Create a new branch:
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. Make your changes following the architecture guidelines

3. Add tests for your feature

4. Run pre-commit checks:
   ```bash
   make pre-commit
   ```

5. Commit and push your changes

### Running a Specific Test

```bash
# Run a specific test by name
cargo test test_name

# Run tests in a specific module
cargo test extractor::tests
```

### Debugging

#### Rust Debugging

Add debug output to your code:

```rust
use log::debug;

debug!("Processing URL: {}", url);
```

Run with debug logging:

```bash
RUST_LOG=debug cargo test
```

#### WebAssembly Debugging

1. Build with debug symbols:
   ```bash
   make wasm-build-debug
   ```

2. Use browser developer tools to inspect console output

3. Add console logging in WASM code:
   ```rust
   web_sys::console::log_1(&"Debug message".into());
   ```

### Performance Testing

Run benchmarks to ensure your changes don't regress performance:

```bash
make bench
```

Compare benchmark results:

```bash
cargo bench -- --save-baseline before
# Make your changes
cargo bench -- --baseline before
```

## Troubleshooting

### Compilation Errors

If you encounter compilation errors:

1. Ensure you have the latest Rust version:
   ```bash
   rustup update
   ```

2. Clean and rebuild:
   ```bash
   make clean
   make build
   ```

### WebAssembly Build Failures

If WASM builds fail:

1. Check that you have the correct target:
   ```bash
   rustup target list --installed | grep wasm32
   ```

2. Ensure wasm-pack is installed:
   ```bash
   which wasm-pack || cargo install wasm-pack
   ```

3. Check for getrandom compatibility:
   - The Makefile sets the correct RUSTFLAGS automatically

### Test Failures

If tests fail:

1. Run tests with verbose output:
   ```bash
   make test-verbose
   ```

2. Run a specific failing test:
   ```bash
   cargo test failing_test_name -- --nocapture
   ```

## IDE Setup

### VS Code

Install recommended extensions:
- rust-analyzer
- CodeLLDB (for debugging)

Create `.vscode/settings.json`:

```json
{
    "rust-analyzer.cargo.features": "all",
    "rust-analyzer.checkOnSave.command": "clippy"
}
```

### IntelliJ IDEA / CLion

Install the Rust plugin and configure:
1. Set Rust toolchain to stable
2. Enable clippy for on-save checks
3. Configure rustfmt for code formatting

## Next Steps

- Read the [Architecture Overview](./architecture.md) to understand the codebase
- Check out [Usage Examples](./examples.md) for implementation patterns
- See [Extractor Development Guide](./extractor-guide.md) to add site-specific extractors
- Review the [API Reference](./api-reference.md) for detailed documentation