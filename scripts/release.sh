#!/usr/bin/env bash
set -euo pipefail

fail() { printf '%s\n' "$*" >&2; exit 1; }

[[ $# == 1 && $1 =~ ^[0-9a-f]{40}$ ]] || fail 'Usage: bash scripts/release.sh <full commit SHA>'
sha=$1
repo=tpellet/grevi
cd "$(git rev-parse --show-toplevel)"
[[ $(gh repo view --json nameWithOwner --jq .nameWithOwner) == "$repo" ]] || fail "Expected repository $repo"
git fetch origin main --tags
git merge-base --is-ancestor "$sha" origin/main || fail 'Commit must belong to origin/main'

# Read the committed manifest: the shared working tree may contain unrelated edits.
version=$(git show "$sha:Cargo.toml" | awk '
  /^\[package\]$/ { package = 1; next }
  /^\[/ { package = 0 }
  package && /^version = "/ { gsub(/"/, "", $3); print $3 }
')
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail 'Expected a stable package version'
tag=v$version
if git show-ref --verify --quiet "refs/tags/$tag"; then
  fail "Tag $tag already exists; inspect its release instead of retagging"
fi
registry_status=$(curl --silent --show-error --user-agent 'grevi-release (https://github.com/tpellet/grevi)' \
  --output /dev/null --write-out '%{http_code}' \
  "https://crates.io/api/v1/crates/grevi/$version")
[[ $registry_status == 404 ]] || fail "Registry version check returned $registry_status; expected unpublished version"

conclusion=$(gh run list --repo "$repo" --workflow ci.yml --branch main --event push \
  --commit "$sha" --limit 1 --json conclusion --jq '.[0].conclusion // "missing"')
[[ $conclusion == success ]] || fail "CI for $sha is $conclusion; a successful branch run is required"

base=$(git describe --tags --match 'v[0-9]*' --abbrev=0 "$sha")
gitleaks git . --log-opts="$base..$sha" --redact
printf 'Releasing %s from %s (review git diff %s..%s before invoking).\n' "$tag" "$sha" "$base" "$sha"
git tag -s "$tag" "$sha" -m "Release $tag"
git push origin "refs/tags/$tag"
