# Top-level Makefile for common TrUAPI dev tasks.
#
# Run `make help` for the list of targets.

.DEFAULT_GOAL := help
.PHONY: help setup build codegen test check playground wasm uniffi android-publish-local

TRUAPI_PKG := js/packages/truapi
PLAYGROUND := playground
JS_PACKAGES := js/packages
WASM_DIST := $(JS_PACKAGES)/truapi-host-shared/dist/wasm

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
	cd $(JS_PACKAGES)/truapi-host-shared && npm install --no-fund --no-audit && npm run build
	cd $(JS_PACKAGES)/truapi-host-web && npm install --no-fund --no-audit && npm run build
	cd $(JS_PACKAGES)/truapi-host-electron && npm install --no-fund --no-audit && npm run build

codegen: ## Regenerate the TypeScript client from the Rust crate.
	./scripts/codegen.sh
	cd $(PLAYGROUND) && rm -rf node_modules/@parity && yarn install

wasm: ## Rebuild the truapi-server WASM artifacts under js/packages/truapi-host-shared/dist/wasm/.
	cd rust/crates/truapi-server && wasm-pack build --target web --no-default-features --out-dir ../../../$(WASM_DIST)/web
	cd rust/crates/truapi-server && wasm-pack build --target nodejs --no-default-features --out-dir ../../../$(WASM_DIST)/node

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

android-publish-local: ## Publish io.parity:truapi-host-android to ~/.m2 (dev workflow).
	gradle :truapi-host:publishReleasePublicationToMavenLocal --no-daemon

test: ## Run Rust + TypeScript client tests.
	cargo test --workspace --features ws-bridge
	cd $(TRUAPI_PKG) && npm test
	cd $(JS_PACKAGES)/truapi-host-shared && npm test
	cd $(JS_PACKAGES)/truapi-host-web && npm test
	cd $(JS_PACKAGES)/truapi-host-electron && npm test

check: ## Full verification suite (build, fmt, clippy, test, TS tests, playground build + lint).
	cargo build --workspace
	cargo +nightly fmt --check
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	cargo test --workspace --features ws-bridge
	cd $(TRUAPI_PKG) && npm run build && npm test
	cd $(JS_PACKAGES)/truapi-host-shared && npm install --no-fund --no-audit && npm test
	cd $(JS_PACKAGES)/truapi-host-web && npm install --no-fund --no-audit && npm test
	cd $(JS_PACKAGES)/truapi-host-electron && npm install --no-fund --no-audit && npm test
	cd $(PLAYGROUND) && yarn build && yarn lint

playground: ## Refresh the playground's @parity/truapi snapshot and rebuild.
	cd $(TRUAPI_PKG) && npm run build
	cd $(PLAYGROUND) && rm -rf node_modules/@parity && yarn install
	cd $(PLAYGROUND) && yarn build
