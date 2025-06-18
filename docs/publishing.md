# Publishing Trek to NPM

This guide explains how to publish Trek to the npm registry.

## Prerequisites

- NPM account with access to publish under `@officialunofficial` scope
- `NPM_PACKAGE_KEY` secret configured in GitHub repository settings
- Rust and wasm-pack installed locally

## Automated Publishing (Recommended)

Trek uses GitHub Actions to automatically publish to npm when a new release is created.

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

4. **Monitor the workflow:**
   - The npm publish workflow will trigger automatically
   - Check [Actions tab](https://github.com/officialunofficial/trek/actions) for progress
   - Package will be available at https://www.npmjs.com/package/@officialunofficial/trek

## Manual Publishing

If you need to publish manually:

### Local Setup:

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

You can also trigger the publish workflow manually:

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

- **Package name:** `@officialunofficial/trek`
- **Registry:** https://registry.npmjs.org
- **Scope:** `@officialunofficial`
- **Access:** Public

## Checking Published Versions

View all published versions:

```bash
npm view @officialunofficial/trek versions
```

View latest version details:

```bash
npm view @officialunofficial/trek
```

## Security Notes

- Never commit npm tokens to the repository
- Use GitHub Secrets for CI/CD authentication
- Regularly rotate npm access tokens
- Enable 2FA on your npm account