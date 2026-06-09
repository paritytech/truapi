# Top-level Makefile for common TrUAPI dev tasks.
#
# Run `make help` for the list of targets.

.DEFAULT_GOAL := help
.PHONY: help setup build codegen test check playground wasm wasm-crypto-test uniffi android-publish-local dev dev-bootstrap dev-link-check matrix explorer

TRUAPI_PKG := js/packages/truapi
PLAYGROUND := playground
JS_PACKAGES := js/packages
EXPLORER := explorer
DOTLI := hosts/dotli
HOST_WASM_PKG := $(JS_PACKAGES)/truapi-host-wasm
HOST_WASM_GENERATED := $(HOST_WASM_PKG)/src/generated/host-callbacks.ts
HOST_WASM_WEB := $(HOST_WASM_PKG)/dist/wasm/web/truapi_server.js
HOST_WASM_NODE := $(HOST_WASM_PKG)/dist/wasm/node/truapi_server.js
DOTLI_UI := $(DOTLI)/packages/ui
DOTLI_HOST_WASM_LINK := $(DOTLI_UI)/node_modules/@parity/truapi-host-wasm

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
	npm ci
	cd $(PLAYGROUND) && yarn install --frozen-lockfile
	cd $(DOTLI) && bun install --frozen-lockfile

build: ## Build the Rust workspace and the TypeScript client.
	cargo build --workspace
	cd $(TRUAPI_PKG) && npm run build
	cd $(HOST_WASM_PKG) && npm run build

codegen: ## Regenerate generated TS/Rust artifacts from the Rust crates.
	./scripts/codegen.sh
	cd $(PLAYGROUND) && rm -rf node_modules/@parity && yarn install

wasm: ## Rebuild the truapi-server WASM artifacts under js/packages/truapi-host-wasm/dist/wasm/.
	cd $(HOST_WASM_PKG) && npm run build:wasm

wasm-crypto-test: ## Run crypto/vector tests on wasm32 via wasm-pack/node.
	wasm-pack test --node rust/crates/truapi-server --test wasm_crypto_vectors --no-default-features

UNIFFI_CDYLIB_DIR := target/release
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
UNIFFI_CDYLIB := $(UNIFFI_CDYLIB_DIR)/libtruapi_server.dylib
else
UNIFFI_CDYLIB := $(UNIFFI_CDYLIB_DIR)/libtruapi_server.so
endif

UNIFFI_SWIFT_TMP := target/uniffi-swift-out

uniffi: ## Regenerate Kotlin + Swift bindings from truapi-server cdylib.
	cargo build -p truapi-server --release --features ws-bridge
	cargo run -p uniffi-bindgen-cli -- generate \
		--library $(UNIFFI_CDYLIB) \
		--language kotlin \
		--out-dir android/truapi-host/src/main/kotlin/generated
	rm -rf $(UNIFFI_SWIFT_TMP)
	mkdir -p $(UNIFFI_SWIFT_TMP)
	cargo run -p uniffi-bindgen-cli -- generate \
		--library $(UNIFFI_CDYLIB) \
		--language swift \
		--out-dir $(UNIFFI_SWIFT_TMP)
	cp $(UNIFFI_SWIFT_TMP)/truapi_server.swift \
		ios/truapi-host/Sources/TrUAPIHost/truapi_server.swift
	cp $(UNIFFI_SWIFT_TMP)/truapi_serverFFI.h \
		ios/truapi-host/Sources/truapi_serverFFI/include/truapi_serverFFI.h
	cp $(UNIFFI_SWIFT_TMP)/truapi_serverFFI.modulemap \
		ios/truapi-host/Sources/truapi_serverFFI/include/module.modulemap

android-publish-local: uniffi ## Publish io.parity:truapi-host-android to ~/.m2 (dev workflow).
	gradle :truapi-host:publishReleasePublicationToMavenLocal --no-daemon

test: ## Run Rust + TypeScript client tests.
	cargo test --workspace --features ws-bridge
	cd $(TRUAPI_PKG) && npm test
	cd $(JS_PACKAGES)/truapi-host-wasm && npm test

check: ## Full verification suite (build, fmt, clippy, test, TS tests, playground build + lint).
	cargo build --workspace
	cargo +nightly fmt --check
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	cargo test --workspace --features ws-bridge
	cd $(TRUAPI_PKG) && npm run build && npm test
	cd $(JS_PACKAGES)/truapi-host-wasm && npm install --no-fund --no-audit && npm test
	cd $(PLAYGROUND) && yarn build && yarn lint

playground: ## Refresh the playground's @parity/truapi snapshot and rebuild.
	cd $(TRUAPI_PKG) && npm run build
	cd $(PLAYGROUND) && rm -rf node_modules/@parity && yarn install
	cd $(PLAYGROUND) && yarn build

dev-bootstrap: ## Prepare ignored generated/build artifacts needed by dotli preview.
	git submodule update --init --recursive
	if [ ! -d node_modules ]; then npm ci; fi
	if [ ! -f "$(HOST_WASM_GENERATED)" ]; then ./scripts/codegen.sh; fi
	cd $(HOST_WASM_PKG) && npm run build
	if [ ! -f "$(HOST_WASM_WEB)" ] || [ ! -f "$(HOST_WASM_NODE)" ]; then $(MAKE) wasm; fi
	cd $(PLAYGROUND) && yarn install --frozen-lockfile
	cd $(DOTLI) && bun install --frozen-lockfile
	$(MAKE) dev-link-check

dev-link-check: ## Verify dotli can resolve the local @parity/truapi-host-wasm package.
	@test -f "$(HOST_WASM_GENERATED)" || (echo "Missing generated host callbacks. Run: make codegen"; exit 1)
	@test -f "$(HOST_WASM_PKG)/dist/index.js" || (echo "Missing @parity/truapi-host-wasm dist. Run: npm run build --prefix $(HOST_WASM_PKG)"; exit 1)
	@test -f "$(HOST_WASM_WEB)" || (echo "Missing @parity/truapi-host-wasm web WASM glue. Run: make wasm"; exit 1)
	@test -e "$(DOTLI_HOST_WASM_LINK)/package.json" || (echo "dotli cannot resolve @parity/truapi-host-wasm. Run top-level: make dev"; exit 1)
	cd $(DOTLI_UI) && bun -e 'await import("@parity/truapi-host-wasm"); await import("@parity/truapi-host-wasm/web");'

dev: dev-bootstrap ## Start dotli host (:5173) + playground (:3000) together; open http://localhost:5173/localhost:3000. DEBUG=1 logs wire frames.
	@trap 'kill 0' EXIT; \
	( cd $(DOTLI) && bun run $(DOTLI_PREVIEW) ) & \
	( cd $(PLAYGROUND) && yarn dev ) & \
	wait

matrix: ## Regenerate the host compatibility matrix from explorer/diagnosis-reports.
	cd $(EXPLORER) && npm run generate-matrix

explorer: ## Run the explorer dev server standalone at http://localhost:5181.
	cd $(EXPLORER) && npx vite --base / --port 5181
