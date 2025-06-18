#!/bin/bash

# Script to bump version numbers in both Cargo.toml and package.json
# Usage: ./scripts/bump-version.sh [major|minor|patch]

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

# Check if version type is provided
if [ -z "$1" ]; then
    echo -e "${RED}Error: Version type not specified${NC}"
    echo "Usage: $0 [major|minor|patch]"
    exit 1
fi

VERSION_TYPE=$1

# Validate version type
if [[ ! "$VERSION_TYPE" =~ ^(major|minor|patch)$ ]]; then
    echo -e "${RED}Error: Invalid version type '$VERSION_TYPE'${NC}"
    echo "Version type must be one of: major, minor, patch"
    exit 1
fi

# Get current version from Cargo.toml
CURRENT_VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
echo -e "${YELLOW}Current version: $CURRENT_VERSION${NC}"

# Parse version components
IFS='.' read -r -a VERSION_PARTS <<< "$CURRENT_VERSION"
MAJOR="${VERSION_PARTS[0]}"
MINOR="${VERSION_PARTS[1]}"
PATCH="${VERSION_PARTS[2]}"

# Bump version based on type
case $VERSION_TYPE in
    major)
        MAJOR=$((MAJOR + 1))
        MINOR=0
        PATCH=0
        ;;
    minor)
        MINOR=$((MINOR + 1))
        PATCH=0
        ;;
    patch)
        PATCH=$((PATCH + 1))
        ;;
esac

NEW_VERSION="$MAJOR.$MINOR.$PATCH"
echo -e "${GREEN}New version: $NEW_VERSION${NC}"

# Update Cargo.toml
sed -i.bak "s/^version = \".*\"/version = \"$NEW_VERSION\"/" Cargo.toml
rm Cargo.toml.bak

# Update Cargo.lock
cargo update --package trek

# Build WASM to generate updated package.json
echo -e "${YELLOW}Building WASM package...${NC}"
make wasm-build

# Verify package.json was updated
PKG_VERSION=$(grep '"version"' pkg/package.json | cut -d'"' -f4)
if [ "$PKG_VERSION" != "$NEW_VERSION" ]; then
    echo -e "${RED}Error: package.json version mismatch${NC}"
    echo "Expected: $NEW_VERSION, Got: $PKG_VERSION"
    exit 1
fi

echo -e "${GREEN}Version bumped successfully to $NEW_VERSION${NC}"
echo ""
echo "Next steps:"
echo "1. Commit the changes: git add -A && git commit -m \"chore: bump version to $NEW_VERSION\""
echo "2. Create a git tag: git tag v$NEW_VERSION"
echo "3. Push changes: git push && git push --tags"
echo "4. Create a GitHub release to trigger automated publishing to:"
echo "   - crates.io (Rust package)"
echo "   - npm (@officialunofficial/trek)"