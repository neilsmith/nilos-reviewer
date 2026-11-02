#!/usr/bin/env bash
# Bump the workspace version, commit, tag v<version>, and push — the tag push
# triggers the release workflow. Usage: scripts/release.sh 0.1.1
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

new_version="${1:-}"
if [[ ! "$new_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "usage: $0 <new-version> (e.g. 0.1.1)" >&2
    exit 2
fi
tag="v$new_version"

if git rev-parse --verify --quiet "refs/tags/$tag" >/dev/null; then
    echo "error: tag $tag already exists" >&2
    exit 1
fi
if [[ -z "$(git symbolic-ref -q HEAD || true)" ]]; then
    echo "error: detached HEAD — check out a branch first" >&2
    exit 1
fi
if [[ -n "$(git status --porcelain)" ]]; then
    echo "error: working tree is dirty — commit or stash first" >&2
    exit 1
fi

# Bump `version` inside [package] of every crate manifest (dependency tables
# can also carry `version` keys, so this is section-scoped, not a blanket sub).
for manifest in Cargo.toml crates/pr-core/Cargo.toml crates/pr-github/Cargo.toml; do
    awk -v v="$new_version" '
        /^\[/ { in_pkg = ($0 == "[package]") }
        in_pkg && /^version = / { sub(/"[^"]*"/, "\"" v "\"") }
        { print }
    ' "$manifest" > "$manifest.tmp" && mv "$manifest.tmp" "$manifest"
done

# Refresh Cargo.lock — release CI builds with --locked.
cargo metadata --format-version 1 >/dev/null

git add Cargo.toml Cargo.lock crates/pr-core/Cargo.toml crates/pr-github/Cargo.toml
git commit -m "Bump version to $tag"
git tag -a "$tag" -m "$tag"
git push origin HEAD "refs/tags/$tag"

echo "Tagged and pushed $tag — the release workflow is building."
