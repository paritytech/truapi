# Top-level Makefile for common TrUAPI dev tasks.
#
# Run `make help` for the list of targets.

.DEFAULT_GOAL := help
.PHONY: help setup build codegen test check clean playground wasm wasm-crypto-test uniffi uniffi-kotlin ios-build ios-run ios-chat-run ios-chat-host-playground-run ios-chat-all android-jni android-publish-local dotli-link dev dev-bootstrap dev-link-check e2e-dotli e2e-signing-cli e2e-pairing-cli headless install matrix explorer xcframework

CARGO ?= cargo
TRUAPI_PKG := js/packages/truapi
PLAYGROUND := playground
JS_PACKAGES := js/packages
EXPLORER := explorer
DOTLI := hosts/dotli
HOST_WASM_PKG := $(JS_PACKAGES)/truapi-host
HOST_CALLBACKS_GENERATED := $(HOST_WASM_PKG)/src/generated/host-callbacks.ts
HOST_WASM_ADAPTER_GENERATED := $(HOST_WASM_PKG)/src/generated/host-callbacks-adapter.ts
HOST_WASM_WORKER_CALLBACKS_GENERATED := $(HOST_WASM_PKG)/src/generated/worker-callbacks.ts
HOST_WASM_WEB := $(HOST_WASM_PKG)/dist/wasm/web/truapi_server.js
DOTLI_UI := $(DOTLI)/packages/ui
DOTLI_NODE_MODULES := $(DOTLI)/node_modules
DOTLI_TRUAPI_LINK := $(DOTLI_NODE_MODULES)/@parity/truapi
DOTLI_HOST_WASM_LINK := $(DOTLI_NODE_MODULES)/@parity/truapi-host
DOTLI_UI_TRUAPI_SHADOW := $(DOTLI_UI)/node_modules/@parity/truapi
DOTLI_UI_HOST_WASM_SHADOW := $(DOTLI_UI)/node_modules/@parity/truapi-host
VITE_NETWORKS ?= paseo-next-v2,previewnet
export VITE_NETWORKS

# Local product URLs (`http://localhost:5173/localhost:3000`) are intentionally
# gated behind dotli's debug build flag, so the dev target must run the debug
# preview by default. Override with `DOTLI_PREVIEW=preview` to test production
# preview behavior.
DOTLI_PREVIEW ?= preview:debug

help: ## Show this help.
	@awk 'BEGIN { FS = ":.*##"; printf "Usage: make <target>\n\nTargets:\n" } \
	      /^[a-zA-Z0-9_-]+:.*?##/ { printf "  %-12s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

setup: ## First-time setup: submodules, JS dependencies, generated artifacts.
	git submodule update --init --recursive
	# --ignore-scripts: the workspace `prepare` builds need generated sources
	# that only exist after codegen.sh, which also builds the packages.
	npm ci --ignore-scripts
	./scripts/codegen.sh
	$(MAKE) uniffi
	cd $(PLAYGROUND) && yarn install --frozen-lockfile
	cd $(DOTLI) && bun install --frozen-lockfile
	$(MAKE) dotli-link

build: ## Build the Rust workspace and the TypeScript client.
	cargo build --workspace
	cd $(TRUAPI_PKG) && npm run build
	cd $(HOST_WASM_PKG) && npm run build

headless: ## Build the truapi-host CLI and generated TypeScript client.
	# The client build shells out to tsc, which `ensure-generated.sh` looks for at
	# the root or in the package. Install workspace deps when neither is present so
	# this target works on a checkout that has not run `make setup`.
	@[ -x node_modules/.bin/tsc ] || [ -x $(TRUAPI_PKG)/node_modules/.bin/tsc ] \
		|| npm ci --ignore-scripts
	cargo build -p truapi-host-cli
	cd $(TRUAPI_PKG) && npm run build

install: headless ## Install the truapi-host CLI into Cargo's bin dir; use as `make headless install`.
	cargo install --path rust/crates/truapi-host-cli --bin truapi-host --locked --force

codegen: ## Regenerate generated TS/Rust artifacts from the Rust crates.
	./scripts/codegen.sh
	cd $(PLAYGROUND) && rm -rf node_modules/@parity && yarn install

wasm: ## Rebuild the truapi-server WASM artifacts under js/packages/truapi-host/dist/wasm/.
	cd $(HOST_WASM_PKG) && npm run build:wasm

wasm-crypto-test: ## Run crypto/vector tests on wasm32 via wasm-pack/node.
	wasm-pack test --node rust/crates/truapi-server --test wasm_crypto_vectors --no-default-features

dotli-link: ## Link dotli to this checkout's local @parity/truapi packages.
	cd $(DOTLI) && TRUAPI_REPO="$(CURDIR)" bun run link:truapi

# uniffi-bindgen scans the cdylib's metadata symbols, which `release` strips, so
# codegen builds use the unstripped `codegen` profile (see [profile.codegen]).
UNIFFI_CDYLIB_DIR := target/codegen
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
UNIFFI_CDYLIB := $(UNIFFI_CDYLIB_DIR)/libtruapi_server.dylib
else
UNIFFI_CDYLIB := $(UNIFFI_CDYLIB_DIR)/libtruapi_server.so
endif

UNIFFI_SWIFT_TMP := target/uniffi-swift-out

uniffi: ## Generate Swift bindings from the truapi-server cdylib into target/uniffi-swift-out (consumed by ios/truapi-host/scripts/rebuild.sh).
	$(CARGO) build -p truapi-server --profile codegen --features ws-bridge
	rm -rf $(UNIFFI_SWIFT_TMP)
	mkdir -p $(UNIFFI_SWIFT_TMP)
	$(CARGO) run -p uniffi-bindgen-cli -- generate \
		--library $(UNIFFI_CDYLIB) \
		--language swift \
		--out-dir $(UNIFFI_SWIFT_TMP)

IOS_HOST ?= ../polkadot-app-ios-v2
IOS_DERIVED_DATA ?= $(IOS_HOST)/build/DerivedData
IOS_CONFIGURATION ?= Debug
IOS_SWIFT_FLAGS ?= -DNIGHTLY -DW3S -DIOS_PASEO_E2E
IOS_SIMULATOR_DEVICE ?=
IOS_XCODE_DESTINATION ?= generic/platform=iOS Simulator
IOS_BUNDLE ?= io.pcf.polkadotapp.develop
IOS_GOOGLE_SERVICE_PLIST ?= $(IOS_HOST)/polkadot-app/GoogleService/GoogleService-Info-Release.plist
IOS_PRODUCT_HOST ?= truapi-playground.dot
IOS_PRODUCT_URL ?= http://localhost:3100
IOS_CHAT_PRODUCT_DIR ?= playground
IOS_CHAT_PRODUCT_HOST ?= truapi-playground.dot
IOS_CHAT_PRODUCT_NAME ?= TrUAPI Playground
IOS_CHAT_PRODUCT_URL ?= http://127.0.0.1:3100
IOS_HOST_PLAYGROUND_DIR ?= ../host-playground
IOS_HOST_PLAYGROUND_HOST ?= host-playground.dot
IOS_HOST_PLAYGROUND_NAME ?= Host Playground
IOS_HOST_PLAYGROUND_URL ?= http://127.0.0.1:3101
IOS_APP := $(abspath $(IOS_DERIVED_DATA)/Build/Products/$(IOS_CONFIGURATION)-iphonesimulator/polkadot-app.app)

ios-build: ## Rebuild the local Rust package and the TestFlight-configured iOS simulator app.
	@test -d "$(IOS_HOST)/.git" || { \
		echo "Missing iOS checkout at $(IOS_HOST); set IOS_HOST to polkadot-app-ios-v2"; \
		exit 1; \
	}
	./ios/truapi-host/scripts/rebuild.sh
	cd $(IOS_HOST) && \
		TRUAPI_LOCAL_PATH="$(CURDIR)" \
		TRUAPI_USE_LOCAL_BINARY=1 \
		RUN_IN_CI=true xcodebuild \
		-project polkadot-app.xcodeproj \
		-scheme polkadot-app \
		-configuration $(IOS_CONFIGURATION) \
		-destination '$(IOS_XCODE_DESTINATION)' \
		-derivedDataPath $(abspath $(IOS_DERIVED_DATA)) \
		ARCHS=arm64 \
		ONLY_ACTIVE_ARCH=YES \
		BASE_SWIFT_FLAGS='$(IOS_SWIFT_FLAGS)' \
		clean build
	cp "$(IOS_GOOGLE_SERVICE_PLIST)" "$(IOS_APP)/GoogleService-Info.plist"
	codesign --force --sign - --preserve-metadata=entitlements "$(IOS_APP)"

ios-run: ios-build ## Build and launch the local TrUAPI playground in an iPhone simulator.
	TRUAPI_IOS_E2E_DEVICE="$(IOS_SIMULATOR_DEVICE)" \
	TRUAPI_IOS_E2E_APP="$(IOS_APP)" \
	TRUAPI_IOS_E2E_BUNDLE="$(IOS_BUNDLE)" \
	TRUAPI_IOS_E2E_PRODUCT_HOST="$(IOS_PRODUCT_HOST)" \
	TRUAPI_IOS_E2E_PRODUCT_URL="$(IOS_PRODUCT_URL)" \
	node scripts/launch-ios-playground.mjs

ios-chat-run: ios-build ## Run the TrUAPI Playground Chat diagnosis in an iPhone simulator.
	TRUAPI_IOS_E2E_DEVICE="$(IOS_SIMULATOR_DEVICE)" \
	TRUAPI_IOS_E2E_APP="$(IOS_APP)" \
	TRUAPI_IOS_E2E_BUNDLE="$(IOS_BUNDLE)" \
	TRUAPI_IOS_E2E_CHAT_PRODUCT_DIR="$(IOS_CHAT_PRODUCT_DIR)" \
	TRUAPI_IOS_E2E_CHAT_PRODUCT_HOST="$(IOS_CHAT_PRODUCT_HOST)" \
	TRUAPI_IOS_E2E_CHAT_PRODUCT_NAME="$(IOS_CHAT_PRODUCT_NAME)" \
	TRUAPI_IOS_E2E_CHAT_PRODUCT_URL="$(IOS_CHAT_PRODUCT_URL)" \
	node scripts/launch-ios-chat-playground.mjs

ios-chat-host-playground-run: ios-build ## Verify Host Playground Chat through the workspace-linked TrUAPI client.
	TRUAPI_IOS_E2E_DEVICE="$(IOS_SIMULATOR_DEVICE)" \
	TRUAPI_IOS_E2E_APP="$(IOS_APP)" \
	TRUAPI_IOS_E2E_BUNDLE="$(IOS_BUNDLE)" \
	TRUAPI_IOS_E2E_CHAT_PRODUCT_DIR="$(abspath $(IOS_HOST_PLAYGROUND_DIR))" \
	TRUAPI_IOS_E2E_CHAT_PRODUCT_HOST="$(IOS_HOST_PLAYGROUND_HOST)" \
	TRUAPI_IOS_E2E_CHAT_PRODUCT_NAME="$(IOS_HOST_PLAYGROUND_NAME)" \
	TRUAPI_IOS_E2E_CHAT_PRODUCT_URL="$(IOS_HOST_PLAYGROUND_URL)" \
	TRUAPI_IOS_E2E_CHAT_ROOM_ID="host-playground-room" \
	TRUAPI_IOS_E2E_CHAT_MESSAGE="!flip" \
	TRUAPI_IOS_E2E_CHAT_EXPECTED_REPLY="Flipping the coin!" \
	TRUAPI_IOS_E2E_CHAT_DIAGNOSIS="0" \
	TRUAPI_IOS_E2E_CHAT_EXPECTED_STARTUP_MESSAGE="" \
	TRUAPI_IOS_E2E_CHAT_EXPECT_CUSTOM_RENDERER="1" \
	TRUAPI_IOS_E2E_CHAT_SCREENSHOT="artifacts/host-playground-coin-flip-chat.png" \
	TRUAPI_IOS_E2E_CHAT_TRUAPI_DIR="$(abspath js/packages/truapi)" \
	node scripts/launch-ios-chat-playground.mjs

ios-chat-all: ios-chat-run ios-chat-host-playground-run ## Run both local iOS Chat playground integrations.

UNIFFI_KOTLIN_OUT := android/truapi-host/src/main/kotlin/generated

uniffi-kotlin: ## Regenerate Kotlin UniFFI bindings from the truapi-server cdylib.
	$(CARGO) build -p truapi-server --profile codegen --features ws-bridge
	rm -rf $(UNIFFI_KOTLIN_OUT)
	mkdir -p $(UNIFFI_KOTLIN_OUT)
	$(CARGO) run -p uniffi-bindgen-cli -- generate \
		--library $(UNIFFI_CDYLIB) \
		--language kotlin \
		--out-dir $(UNIFFI_KOTLIN_OUT)

# Android ABIs to cross-compile the cdylib for. arm64 + armv7 cover physical
# devices; x86_64 covers the emulator on Intel/Apple-silicon hosts.
ANDROID_ABIS ?= arm64-v8a armeabi-v7a x86_64
ANDROID_JNILIBS := android/truapi-host/src/main/jniLibs

android-jni: ## Cross-compile libtruapi_server.so for Android ABIs into jniLibs (needs cargo-ndk + NDK).
	@command -v cargo-ndk >/dev/null || { echo "cargo-ndk not found: cargo install cargo-ndk"; exit 1; }
	$(CARGO) ndk $(foreach abi,$(ANDROID_ABIS),-t $(abi)) \
		-o $(ANDROID_JNILIBS) \
		build --release -p truapi-server --features ws-bridge

android-publish-local: uniffi-kotlin ## Generate Kotlin bindings, then publish the AAR to ~/.m2 (needs Gradle + JDK 17). The AAR does not bundle the cdylib; consumers build it per ABI (see android-jni).
	gradle :truapi-host:publishReleasePublicationToMavenLocal

test: ## Run Rust + TypeScript client tests.
	cargo test --workspace
	cd $(TRUAPI_PKG) && npm test
	cd $(JS_PACKAGES)/truapi-host && npm test

check: ## Full verification suite (build, fmt, clippy, test, TS tests, playground build + lint).
	cargo build --workspace
	cargo check --target wasm32-unknown-unknown -p truapi-server
	cargo +nightly fmt --check
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	cargo test --workspace --all-features --all-targets
	cd $(TRUAPI_PKG) && npm run build && npm test
	cd $(JS_PACKAGES)/truapi-host && npm install --no-fund --no-audit && npm test
	cd $(PLAYGROUND) && yarn build && yarn lint

clean: ## Remove local build/test artifacts without deleting dependencies.
	cargo clean
	rm -rf \
		$(TRUAPI_PKG)/dist \
		$(TRUAPI_PKG)/tsconfig.tsbuildinfo \
		$(HOST_WASM_PKG)/dist \
		$(HOST_WASM_PKG)/tsconfig.tsbuildinfo \
		$(PLAYGROUND)/.next \
		$(PLAYGROUND)/out \
		$(PLAYGROUND)/test-results \
		$(PLAYGROUND)/tsconfig.tsbuildinfo \
		$(PLAYGROUND)/tests/tsconfig.tsbuildinfo \
		$(DOTLI)/.turbo \
		$(DOTLI)/apps/host/dist \
		$(DOTLI)/apps/protocol/dist \
		$(DOTLI)/apps/sandbox/dist \
		$(DOTLI)/test-results

playground: ## Refresh the playground's @parity/truapi snapshot and rebuild.
	cd $(TRUAPI_PKG) && npm run build
	cd $(PLAYGROUND) && rm -rf node_modules/@parity && yarn install
	cd $(PLAYGROUND) && yarn build

dev-bootstrap: ## Prepare ignored generated/build artifacts needed by dotli preview.
	git submodule update --init --recursive
	# --ignore-scripts: the workspace `prepare` builds need generated sources
	# that only exist after codegen.sh, which also builds the packages.
	if [ ! -d node_modules ]; then npm ci --ignore-scripts; fi
	./scripts/codegen.sh
	cd $(HOST_WASM_PKG) && npm run build
	TRUAPI_WASM_PROFILE=dev $(MAKE) wasm
	cd $(PLAYGROUND) && yarn install --frozen-lockfile
	cd $(DOTLI) && bun install --frozen-lockfile
	$(MAKE) dev-link-check

dev-link-check: dotli-link ## Verify dotli can resolve the local @parity/truapi-host package.
	@test -f "$(HOST_CALLBACKS_GENERATED)" || (echo "Missing generated host callbacks. Run: make codegen"; exit 1)
	@test -f "$(HOST_WASM_ADAPTER_GENERATED)" || (echo "Missing generated host callbacks WASM adapter. Run: make codegen"; exit 1)
	@test -f "$(HOST_WASM_WORKER_CALLBACKS_GENERATED)" || (echo "Missing generated host callbacks worker bridge. Run: make codegen"; exit 1)
	@test -f "$(HOST_WASM_PKG)/dist/index.js" || (echo "Missing @parity/truapi-host dist. Run: npm run build --prefix $(HOST_WASM_PKG)"; exit 1)
	@test -f "$(HOST_WASM_WEB)" || (echo "Missing @parity/truapi-host web WASM glue. Run: make wasm"; exit 1)
	@test -e "$(DOTLI_TRUAPI_LINK)/package.json" || (echo "dotli cannot resolve @parity/truapi. Run top-level: make dotli-link"; exit 1)
	@test -e "$(DOTLI_HOST_WASM_LINK)/package.json" || (echo "dotli cannot resolve @parity/truapi-host. Run top-level: make dotli-link"; exit 1)
	@test ! -e "$(DOTLI_UI_TRUAPI_SHADOW)/package.json" || (echo "$(DOTLI_UI_TRUAPI_SHADOW) shadows the local workspace link. Run top-level: make dotli-link"; exit 1)
	@test ! -e "$(DOTLI_UI_HOST_WASM_SHADOW)/package.json" || (echo "$(DOTLI_UI_HOST_WASM_SHADOW) shadows the local workspace link. Run top-level: make dotli-link"; exit 1)
	@node -e 'const fs = require("node:fs"); const checks = [["$(DOTLI_TRUAPI_LINK)/package.json", "@parity/truapi"], ["$(DOTLI_HOST_WASM_LINK)/package.json", "@parity/truapi-host"]]; for (const [path, name] of checks) { const pkg = JSON.parse(fs.readFileSync(path, "utf8")); if (pkg.name !== name) { console.error(path + " resolves " + pkg.name + ", expected local " + name + ". Run: make dotli-link"); process.exit(1); } }'
	cd $(DOTLI_UI) && bun -e 'await import("@parity/truapi-host"); await import("@parity/truapi-host/web");'

dev: dev-bootstrap ## Start dotli host (:5173) + playground (:3000) together; open http://localhost:5173/localhost:3000. DEBUG=1 logs wire frames.
	@trap 'kill 0' EXIT; \
	( cd $(DOTLI) && bun run $(DOTLI_PREVIEW) ) & \
	( cd $(PLAYGROUND) && yarn dev ) & \
	( until curl -fsS http://localhost:3000/ >/dev/null 2>&1; do sleep 1; done; curl -fsS http://localhost:3000/diagnostics >/dev/null 2>&1 || true ) & \
	wait

e2e-dotli: ## Fully automated dotli + playground diagnosis e2e using the local signing-host CLI.
	@$(MAKE) dev-bootstrap
	cargo build -p truapi-host-cli
	cd $(PLAYGROUND) && bun tests/e2e/dotli-diagnosis.ts

e2e-signing-cli: ## Run the generated battery against the direct signing-host CLI.
	scripts/battery.sh --signing-host

e2e-pairing-cli: ## Run the generated battery against the paired pairing-host CLI.
	scripts/battery.sh --pairing-host

matrix: ## Regenerate the host compatibility matrix from explorer/diagnosis-reports.
	cd $(EXPLORER) && npm run generate-matrix

explorer: ## Run the explorer dev server standalone at http://localhost:5181.
	cd $(EXPLORER) && npx vite --base / --port 5181

IOS_DEVICE_TARGET := aarch64-apple-ios
IOS_SIM_TARGET := aarch64-apple-ios-sim
# Must match the TrUAPIHost Package.swift platforms entry. Without it rustc/cc
# stamp objects with the SDK version and every consumer link emits
# "built for newer iOS version than being linked" warnings.
IOS_DEPLOYMENT_TARGET := 17.0
XCFRAMEWORK_OUT := target/truapi_server.xcframework
XCFRAMEWORK_HEADERS := target/xcframework-headers

xcframework: uniffi ## Build truapi_server.xcframework for iOS device + simulator.
	rustup target add $(IOS_DEVICE_TARGET) $(IOS_SIM_TARGET)
	IPHONEOS_DEPLOYMENT_TARGET=$(IOS_DEPLOYMENT_TARGET) $(CARGO) build -p truapi-server --release \
		--features ws-bridge --target $(IOS_DEVICE_TARGET)
	IPHONEOS_DEPLOYMENT_TARGET=$(IOS_DEPLOYMENT_TARGET) $(CARGO) build -p truapi-server --release \
		--features ws-bridge --target $(IOS_SIM_TARGET)
	rm -rf $(XCFRAMEWORK_OUT) $(XCFRAMEWORK_HEADERS)
	mkdir -p $(XCFRAMEWORK_HEADERS)
	cp $(UNIFFI_SWIFT_TMP)/truapiFFI.h $(UNIFFI_SWIFT_TMP)/truapi_platformFFI.h \
		$(UNIFFI_SWIFT_TMP)/truapi_serverFFI.h $(XCFRAMEWORK_HEADERS)/
	cp $(UNIFFI_SWIFT_TMP)/truapi_serverFFI.modulemap $(XCFRAMEWORK_HEADERS)/module.modulemap
	xcodebuild -create-xcframework \
		-library target/$(IOS_DEVICE_TARGET)/release/libtruapi_server.a \
		-headers $(XCFRAMEWORK_HEADERS) \
		-library target/$(IOS_SIM_TARGET)/release/libtruapi_server.a \
		-headers $(XCFRAMEWORK_HEADERS) \
		-output $(XCFRAMEWORK_OUT)
