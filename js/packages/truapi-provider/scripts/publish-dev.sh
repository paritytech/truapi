#!/usr/bin/env bash
# Publish a dev-tagged snapshot of @parity/truapi-provider to npm.
#
# For quick iteration ahead of a formal release (which goes through
# .github/workflows/release.yml -> paritytech/npm_publish_automation). Rebuilds
# the wasm bundle, stamps a prerelease version (`<base>-dev.t<utc>.<sha>`),
# publishes it under the `dev` dist-tag so `latest` is never moved, then
# restores the base version in package.json.
#
# Auth comes from an env var, matching CI: export the npm token as
# NODE_AUTH_TOKEN (or NPM_TOKEN) with publish access to the @parity scope.
#
#   NODE_AUTH_TOKEN=<token> npm run publish:dev
#
# dotli then depends on the printed exact version, e.g.
#   "@parity/truapi-provider": "0.1.0-dev.t20260714....<sha>"
set -euo pipefail

cd "$(dirname "$0")/.."

token="${NODE_AUTH_TOKEN:-${NPM_TOKEN:-}}"
if [ -z "$token" ]; then
  echo "error: set NODE_AUTH_TOKEN (or NPM_TOKEN) to an npm token with @parity publish access" >&2
  exit 1
fi

base=$(node -p "require('./package.json').version")
stamp=$(date -u +%Y%m%d%H%M%S)
sha=$(git rev-parse --short HEAD)
version="${base%%-*}-dev.t${stamp}.${sha}"

echo "Building wasm bundle..."
npm run build:wasm

# Scope the token to this publish via a package-local .npmrc, then remove it and
# restore the base version on exit however the script ends.
npmrc="$PWD/.npmrc"
printf '//registry.npmjs.org/:_authToken=%s\n' "$token" > "$npmrc"
cleanup() {
  rm -f "$npmrc"
  npm version --no-git-tag-version --allow-same-version "$base" >/dev/null 2>&1 || true
}
trap cleanup EXIT

npm version --no-git-tag-version --allow-same-version "$version" >/dev/null

echo "Publishing @parity/truapi-provider@${version} (dist-tag: dev)..."
npm publish --access public --tag dev

echo
echo "Published @parity/truapi-provider@${version}"
echo "Pin it in dotli:"
echo "  \"@parity/truapi-provider\": \"${version}\""
