# Contributing to Trek

Thank you for your interest in contributing to Trek! This document provides guidelines and instructions for contributing to the project.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Commit Message Guidelines](#commit-message-guidelines)
- [Pull Request Process](#pull-request-process)
- [Development Workflow](#development-workflow)
- [Testing Guidelines](#testing-guidelines)
- [Documentation](#documentation)

## Code of Conduct

Please note that this project is released with a Contributor Code of Conduct. By participating in this project you agree to abide by its terms.

## Getting Started

1. Fork the repository on GitHub
2. Clone your fork locally
3. Set up the development environment (see below)
4. Create a new branch for your feature or fix
5. Make your changes
6. Submit a pull request

## Development Setup

### Prerequisites

- Rust (stable) - Install via [rustup](https://rustup.rs/)
- Node.js (for npm publishing and scripts)
- Python 3 (for development server)
- wasm-pack (for WebAssembly builds)

### Installing Development Dependencies

```bash
make install-dev-deps
```

This will install:
- cargo-outdated
- cargo-audit
- cargo-tarpaulin
- git-cliff
- wasm-pack

### Setting Up Git

Configure git to use our commit message template:

```bash
git config --local commit.template .gitmessage
```

## Commit Message Guidelines

We use [Conventional Commits](https://www.conventionalcommits.org/) for our commit messages. This enables automatic changelog generation and semantic versioning.

### Commit Message Format

```
<type>(<scope>): <subject>

<body>

<footer>
```

### Types

- **feat**: A new feature
- **fix**: A bug fix
- **docs**: Documentation only changes
- **style**: Changes that do not affect the meaning of the code (formatting, missing semi colons, etc)
- **refactor**: A code change that neither fixes a bug nor adds a feature
- **perf**: A code change that improves performance
- **test**: Adding missing tests or correcting existing tests
- **build**: Changes that affect the build system or external dependencies
- **ci**: Changes to our CI configuration files and scripts
- **chore**: Other changes that don't modify src or test files
- **revert**: Reverts a previous commit

### Scope

The scope is optional and can be anything specifying the place of the commit change. Examples:
- `wasm`
- `extractor`
- `metadata`
- `parser`
- `api`

### Subject

- Use the imperative, present tense: "change" not "changed" nor "changes"
- Don't capitalize the first letter
- No period at the end
- Limit to 50 characters

### Body

- Use the imperative, present tense
- Include motivation for the change and contrasts with previous behavior
- Wrap at 72 characters

### Footer

- Reference GitHub issues: `Closes #123`, `Fixes #456`
- Note breaking changes: `BREAKING CHANGE: description`

### Examples

```
feat(wasm): add support for custom headers

Allow users to pass custom headers when extracting content.
This enables authentication and custom user agents.

Closes #123
```

```
fix(parser): handle empty meta tags correctly

Previously, empty meta tags would cause a panic. Now they are
properly handled and ignored.

Fixes #456
```

```
feat!: remove deprecated extract_simple method

BREAKING CHANGE: The extract_simple method has been removed.
Use extract() with default options instead.
```

## Pull Request Process

1. Ensure all tests pass: `make test`
2. Run the pre-commit checks: `make pre-commit`
3. Update documentation if needed
4. Update the CHANGELOG.md if your changes are user-facing
5. Create a pull request with a clear title and description
6. Link any related issues
7. Wait for code review and address any feedback

### PR Title Format

Pull request titles should follow the same format as commit messages.

## Development Workflow

### Common Commands

```bash
# Check if code compiles
make check

# Run tests
make test

# Format code
make fmt

# Run linter
make clippy

# Run all pre-commit checks
make pre-commit

# Build WebAssembly module
make wasm-build

# Start development server
make serve

# Generate changelog
make changelog
```

### Before Committing

Always run the pre-commit checks:

```bash
make pre-commit
```

This will:
1. Format your code
2. Check compilation
3. Run clippy linter
4. Run all tests

### WebAssembly Development

For WASM-specific development:

```bash
# Build WASM module
make wasm-build

# Build debug WASM module
make wasm-build-debug

# Run WASM tests
make wasm-test

# Start playground server
make playground
```

## Testing Guidelines

### Unit Tests

- Write unit tests for all new functionality
- Place tests in the same file as the code being tested
- Use descriptive test names that explain what is being tested

### Integration Tests

- Add integration tests in the `tests/` directory
- Test the public API and common use cases
- Ensure tests are deterministic and don't rely on external resources

### Running Tests

```bash
# Run all tests
make test

# Run tests with output
make test-verbose

# Run WASM tests
make wasm-test

# Generate coverage report
make coverage
```

## Documentation

### Code Documentation

- Document all public APIs with rustdoc comments
- Include examples in documentation when helpful
- Keep documentation up-to-date with code changes

### Project Documentation

- Update README.md for significant features
- Add guides to the `docs/` directory for complex features
- Update CLAUDE.md if development practices change

### Generating Documentation

```bash
# Generate and open documentation
make doc
```

## Release Process

Releases are managed by maintainers. The process involves:

1. Update version in Cargo.toml
2. Generate changelog: `make changelog`
3. Create a git tag: `git tag v0.x.x`
4. Push tag to trigger release workflow

## Questions?

If you have questions about contributing, feel free to:
- Open an issue for discussion
- Check existing issues and pull requests
- Review the documentation in the `docs/` directory

Thank you for contributing to Trek!