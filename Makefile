# Top-level Makefile for common TrUAPI dev tasks.
#
# Run `make help` for the list of targets.

.DEFAULT_GOAL := help
.PHONY: help setup build codegen test check playground dev matrix explorer dart

TRUAPI_PKG := js/packages/truapi
PLAYGROUND := playground
EXPLORER := explorer
DART := dart/truapi
DOTLI := hosts/dotli

# `make dev DEBUG=1` runs dotli with VITE_APP_DEBUG=true to log every wire frame.
DOTLI_PREVIEW := preview
ifdef DEBUG
DOTLI_PREVIEW := preview:debug
endif

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

dev: ## Start dotli host (:5173) + playground (:3000) together; open http://localhost:5173/localhost:3000. DEBUG=1 logs wire frames.
	@trap 'kill 0' EXIT; \
	( cd $(DOTLI) && bun run $(DOTLI_PREVIEW) ) & \
	( cd $(PLAYGROUND) && yarn dev ) & \
	wait

matrix: ## Regenerate the host compatibility matrix from explorer/diagnosis-reports.
	cd $(EXPLORER) && npm run generate-matrix

explorer: ## Run the explorer dev server standalone at http://localhost:5181.
	cd $(EXPLORER) && npx vite --base / --port 5181

dart: ## Regenerate + analyze + test the Dart client (regen golden vectors too).
	./scripts/codegen.sh
	cargo run -p truapi --example wire_vectors -- $(DART)/test/wire_vectors.json
	cd $(DART) && dart pub get && dart analyze && dart test
