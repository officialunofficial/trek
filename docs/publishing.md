# Publishing Trek

This guide explains how to publish Trek to both crates.io (Rust) and npm (WebAssembly).

## Prerequisites

- NPM account with access to publish under `@officialunofficial` scope
- Crates.io account with publish access
- GitHub repository secrets configured:
  - `NPM_PACKAGE_KEY` - npm authentication token
  - `CRATES_PACKAGE_KEY` - crates.io API token
- Rust and wasm-pack installed locally

## Automated Publishing (Recommended)

Trek uses GitHub Actions to automatically publish to both crates.io and npm when a new release is created. The release workflow will:

1. First publish to crates.io (Rust package)
2. Then publish to npm (WebAssembly package)
3. Finally create release artifacts

### Steps:

1. **Bump the version:**
   ```bash
   ./scripts/bump-version.sh patch  # or minor/major
   ```

2. **Commit and push changes:**
   ```bash
   git add -A
   git commit -m "chore: bump version to X.Y.Z"
   git push
   ```

3. **Create a GitHub release:**
   - Go to [GitHub Releases](https://github.com/officialunofficial/trek/releases)
   - Click "Create a new release"
   - Create a new tag (e.g., `v0.1.1`)
   - Add release notes
   - Click "Publish release"

4. **Monitor the workflows:**
   - The release workflow will trigger automatically
   - Check [Actions tab](https://github.com/officialunofficial/trek/actions) for progress
   - Packages will be available at:
     - Rust: https://crates.io/crates/trek-rs
     - npm: https://www.npmjs.com/package/@officialunofficial/trek

## Manual Publishing

If you need to publish manually:

### Publishing to crates.io:

1. **Login to crates.io:**
   ```bash
   cargo login
   ```

2. **Dry run (recommended):**
   ```bash
   make crates-publish-dry
   ```

3. **Publish:**
   ```bash
   make crates-publish
   ```

### Publishing to npm:

1. **Login to npm:**
   ```bash
   npm login
   ```

2. **Build the WASM package:**
   ```bash
   make wasm-build
   ```

3. **Dry run (recommended):**
   ```bash
   make npm-publish-dry
   ```

4. **Publish:**
   ```bash
   make npm-publish
   ```

### Using GitHub Actions Manually:

You can trigger individual publish workflows manually:

#### For crates.io:
1. Go to [Actions → Publish to crates.io](https://github.com/officialunofficial/trek/actions/workflows/crates-publish.yml)
2. Click "Run workflow"
3. Select options:
   - Branch: `main`
   - Dry run: `true` for testing, `false` for actual publish
4. Click "Run workflow"

#### For npm:
1. Go to [Actions → Publish to NPM](https://github.com/officialunofficial/trek/actions/workflows/npm-publish.yml)
2. Click "Run workflow"
3. Select options:
   - Branch: `main`
   - Dry run: `true` for testing, `false` for actual publish
4. Click "Run workflow"

## Version Management

### Version Sync

The version is maintained in `Cargo.toml` and automatically synced to `pkg/package.json` during the WASM build process.

### Versioning Strategy

Trek follows [Semantic Versioning](https://semver.org/):

- **MAJOR** (X.0.0): Breaking API changes
- **MINOR** (0.X.0): New features, backwards compatible
- **PATCH** (0.0.X): Bug fixes, backwards compatible

### Pre-release Versions

For testing, you can publish pre-release versions:

```bash
# In Cargo.toml
version = "0.2.0-beta.1"

# Build and publish with beta tag
make wasm-build
cd pkg
npm publish --tag beta --access public
```

## Troubleshooting

### Authentication Errors

If you get authentication errors:

1. Ensure `NPM_PACKAGE_KEY` is set in GitHub Secrets
2. Verify the token has publish permissions
3. Check token hasn't expired

### Version Already Exists

If the version already exists on npm:

1. Bump the version number
2. Rebuild the WASM package
3. Try publishing again

### Build Failures

If the WASM build fails:

1. Ensure wasm-pack is installed:
   ```bash
   cargo install wasm-pack
   ```

2. Check Rust toolchain:
   ```bash
   rustup target add wasm32-unknown-unknown
   ```

3. Clean and rebuild:
   ```bash
   make clean
   make wasm-build
   ```

## Package Information

### Rust Package (crates.io)
- **Package name:** `trek-rs`
- **Registry:** https://crates.io
- **Categories:** web-programming, text-processing, wasm
- **Keywords:** html, parsing, extraction, readability, wasm

### npm Package
- **Package name:** `@officialunofficial/trek`
- **Registry:** https://registry.npmjs.org
- **Scope:** `@officialunofficial`
- **Access:** Public

## Checking Published Versions

### Rust (crates.io):

```bash
# View crate info
cargo search trek-rs --limit 1

# View on web
open https://crates.io/crates/trek-rs
```

### npm:

```bash
# View all published versions
npm view @officialunofficial/trek versions

# View latest version details
npm view @officialunofficial/trek
```

## Security Notes

- Never commit API tokens to the repository
- Use GitHub Secrets for CI/CD authentication:
  - `CRATES_PACKAGE_KEY` for crates.io
  - `NPM_PACKAGE_KEY` for npm
- Regularly rotate access tokens
- Enable 2FA on both crates.io and npm accounts
- Review dependencies before publishing