#!/usr/bin/env bash
# Publish a dev-tagged snapshot of @parity/truapi-provider to npm.
#
# Rebuilds the wasm bundle, stamps a prerelease version derived from the base
# version plus a UTC timestamp and the short git sha, publishes it under the
# `dev` dist-tag (so `latest` is never moved), then restores the base version in
# package.json. Requires npm auth with publish access to the @parity scope.
#
#   npm run publish:dev        # from js/packages/truapi-provider
#
# dotli then depends on the printed exact version, e.g.
#   "@parity/truapi-provider": "0.1.0-dev.t20260714....<sha>"
set -euo pipefail

cd "$(dirname "$0")/.."

base=$(node -p "require('./package.json').version")
stamp=$(date -u +%Y%m%d%H%M%S)
sha=$(git rev-parse --short HEAD)
version="${base%%-*}-dev.t${stamp}.${sha}"

echo "Building wasm bundle..."
npm run build:wasm

echo "Publishing @parity/truapi-provider@${version} (dist-tag: dev)..."
npm version --no-git-tag-version --allow-same-version "$version" >/dev/null
trap 'npm version --no-git-tag-version --allow-same-version "$base" >/dev/null' EXIT
npm publish

echo
echo "Published @parity/truapi-provider@${version}"
echo "Pin it in dotli:"
echo "  \"@parity/truapi-provider\": \"${version}\""
