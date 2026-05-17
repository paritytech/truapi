# Top-level Makefile for common TrUAPI dev tasks.
#
# Run `make help` for the list of targets.

.DEFAULT_GOAL := help
.PHONY: help setup build codegen test check playground wasm uniffi

TRUAPI_PKG := js/packages/truapi
PLAYGROUND := playground
HOST_LIBS_JS := host-libs/js
WASM_DIST := $(HOST_LIBS_JS)/shared/dist/wasm

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
	cd $(HOST_LIBS_JS)/shared && npm install --no-fund --no-audit && npm run build
	cd $(HOST_LIBS_JS)/web && npm install --no-fund --no-audit && npm run build
	cd $(HOST_LIBS_JS)/electron && npm install --no-fund --no-audit && npm run build

codegen: ## Regenerate the TypeScript client from the Rust crate.
	./scripts/codegen.sh
	cd $(PLAYGROUND) && rm -rf node_modules/@parity && yarn install

wasm: ## Rebuild the truapi-server WASM artifacts under host-libs/js/shared/dist/wasm/.
	cd rust/crates/truapi-server && wasm-pack build --target web --no-default-features --out-dir ../../../$(WASM_DIST)/web
	cd rust/crates/truapi-server && wasm-pack build --target nodejs --no-default-features --out-dir ../../../$(WASM_DIST)/node

uniffi: ## Regenerate Kotlin + Swift bindings from truapi-server cdylib.
	cargo build -p truapi-server --release --features ws-bridge
	cargo run -p uniffi-bindgen-cli -- generate \
		--library target/release/libtruapi_server.so \
		--language kotlin \
		--out-dir host-libs/android/src/main/kotlin/generated
	cargo run -p uniffi-bindgen-cli -- generate \
		--library target/release/libtruapi_server.so \
		--language swift \
		--out-dir host-libs/ios/TrUAPIHost/Sources/Generated
	@echo "Reminder: the iOS Generated/*.modulemap may need renaming to module.modulemap and moving to Sources/truapi_serverFFI/include/"

test: ## Run Rust + TypeScript client tests.
	cargo test --workspace --features ws-bridge
	cd $(TRUAPI_PKG) && npm test
	cd $(HOST_LIBS_JS)/shared && npm test
	cd $(HOST_LIBS_JS)/web && npm test
	cd $(HOST_LIBS_JS)/electron && npm test

check: ## Full verification suite (build, fmt, clippy, test, TS tests, playground build + lint).
	cargo build --workspace
	cargo +nightly fmt --check
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	cargo test --workspace --features ws-bridge
	cd $(TRUAPI_PKG) && npm run build && npm test
	cd $(HOST_LIBS_JS)/shared && npm install --no-fund --no-audit && npm test
	cd $(HOST_LIBS_JS)/web && npm install --no-fund --no-audit && npm test
	cd $(HOST_LIBS_JS)/electron && npm install --no-fund --no-audit && npm test
	cd $(PLAYGROUND) && yarn build && yarn lint

playground: ## Refresh the playground's @parity/truapi snapshot and rebuild.
	cd $(TRUAPI_PKG) && npm run build
	cd $(PLAYGROUND) && rm -rf node_modules/@parity && yarn install
	cd $(PLAYGROUND) && yarn build
