# Top-level Makefile for common TrUAPI dev tasks.
#
# Run `make help` for the list of targets.

.DEFAULT_GOAL := help
.PHONY: help setup build codegen test check playground

TRUAPI_PKG := js/packages/truapi
PLAYGROUND := playground

help: ## Show this help.
	@awk 'BEGIN { FS = ":.*##"; printf "Usage: make <target>\n\nTargets:\n" } \
	      /^[a-zA-Z_-]+:.*?##/ { printf "  %-12s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

setup: ## First-time setup: submodules + JS dependencies.
	git submodule update --init --recursive
	cd $(TRUAPI_PKG) && npm install
	cd $(PLAYGROUND) && yarn install --frozen-lockfile

build: ## Build the Rust workspace and the TypeScript client.
	cargo build --workspace
	cd $(TRUAPI_PKG) && npm run build

codegen: ## Regenerate the TypeScript client from the Rust crate.
	./scripts/codegen.sh
	cd $(PLAYGROUND) && rm -rf node_modules/@parity && yarn install

test: ## Run Rust + TypeScript client tests.
	cargo test --workspace
	cd $(TRUAPI_PKG) && npm test

check: ## Full verification suite (build, fmt, clippy, test, TS tests, playground build + lint).
	cargo build --workspace
	cargo +nightly fmt --check
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	cargo test --workspace
	cd $(TRUAPI_PKG) && npm run build && npm test
	cd $(PLAYGROUND) && yarn build && yarn lint

playground: ## Refresh the playground's @parity/truapi snapshot and rebuild.
	cd $(TRUAPI_PKG) && npm run build
	cd $(PLAYGROUND) && rm -rf node_modules/@parity && yarn install
	cd $(PLAYGROUND) && yarn build
