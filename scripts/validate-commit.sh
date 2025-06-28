#!/bin/bash

# Validate commit message follows conventional commit format
# Usage: ./scripts/validate-commit.sh <commit-message-file>

commit_regex='^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(\([a-z0-9\-]+\))?: .{1,50}$'
merge_regex='^Merge '

# Read the commit message
commit_message=$(cat "$1" | head -n1)

# Allow merge commits
if [[ "$commit_message" =~ $merge_regex ]]; then
    exit 0
fi

# Check if the commit message matches the conventional format
if ! [[ "$commit_message" =~ $commit_regex ]]; then
    echo "❌ Invalid commit message format!"
    echo ""
    echo "The commit message must follow the Conventional Commits format:"
    echo "<type>(<scope>): <subject>"
    echo ""
    echo "Valid types: feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert"
    echo "Scope is optional and should be lowercase"
    echo "Subject should be max 50 characters, start with lowercase"
    echo ""
    echo "Examples:"
    echo "  feat(wasm): add support for custom headers"
    echo "  fix: handle empty meta tags correctly"
    echo "  docs: update installation instructions"
    echo ""
    echo "Your commit message:"
    echo "  $commit_message"
    echo ""
    echo "Tip: Use 'git commit' without -m to use the commit template"
    exit 1
fi

exit 0