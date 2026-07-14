# Top-level Makefile for common TrUAPI dev tasks.
#
# Run `make help` for the list of targets.

.DEFAULT_GOAL := help
.PHONY: help setup build codegen test check clean playground wasm wasm-crypto-test dotli-link dev dev-bootstrap dev-link-check e2e-dotli matrix explorer

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
SIGNER_BOT_BASE_URL ?= https://signing-bot-dev.novasama-tech.org/
SIGNER_BOT_NETWORK ?= paseo-next-v2
SIGNER_BOT_BASE_URL_ORIGIN := $(origin SIGNER_BOT_BASE_URL)
SIGNER_BOT_NETWORK_ORIGIN := $(origin SIGNER_BOT_NETWORK)
VITE_NETWORKS ?= paseo-next-v2,previewnet
export SIGNER_BOT_BASE_URL
export SIGNER_BOT_NETWORK
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
	cd $(PLAYGROUND) && yarn install --frozen-lockfile
	cd $(DOTLI) && bun install --frozen-lockfile
	$(MAKE) dotli-link

build: ## Build the Rust workspace and the TypeScript client.
	cargo build --workspace
	cd $(TRUAPI_PKG) && npm run build
	cd $(HOST_WASM_PKG) && npm run build

codegen: ## Regenerate generated TS/Rust artifacts from the Rust crates.
	./scripts/codegen.sh
	cd $(PLAYGROUND) && rm -rf node_modules/@parity && yarn install

wasm: ## Rebuild the truapi-server WASM artifacts under js/packages/truapi-host/dist/wasm/.
	cd $(HOST_WASM_PKG) && npm run build:wasm

wasm-crypto-test: ## Run crypto/vector tests on wasm32 via wasm-pack/node.
	wasm-pack test --node rust/crates/truapi-server --test wasm_crypto_vectors --no-default-features

dotli-link: ## Link dotli to this checkout's local @parity/truapi packages.
	cd $(DOTLI) && TRUAPI_REPO="$(CURDIR)" bun run link:truapi

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
	if [ ! -f "$(HOST_CALLBACKS_GENERATED)" ] || [ ! -f "$(HOST_WASM_ADAPTER_GENERATED)" ] || [ ! -f "$(HOST_WASM_WORKER_CALLBACKS_GENERATED)" ]; then ./scripts/codegen.sh; fi
	cd $(TRUAPI_PKG) && npm run build
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

e2e-dotli: ## Fully automated dotli + playground diagnosis e2e. Requires SIGNER_BOT_SVC_TOKEN unless E2E_DOTLI_SMOKE=1.
	@SIGNER_BOT_SVC_TOKEN_ENV="$$SIGNER_BOT_SVC_TOKEN"; \
	SIGNER_BOT_BASE_URL_ENV="$$SIGNER_BOT_BASE_URL"; \
	SIGNER_BOT_NETWORK_ENV="$$SIGNER_BOT_NETWORK"; \
	SIGNER_BOT_BASE_URL_ORIGIN="$(SIGNER_BOT_BASE_URL_ORIGIN)"; \
	SIGNER_BOT_NETWORK_ORIGIN="$(SIGNER_BOT_NETWORK_ORIGIN)"; \
	set -a; \
	if [ -f .env ]; then . ./.env; fi; \
	set +a; \
	if [ -n "$$SIGNER_BOT_SVC_TOKEN_ENV" ]; then SIGNER_BOT_SVC_TOKEN="$$SIGNER_BOT_SVC_TOKEN_ENV"; export SIGNER_BOT_SVC_TOKEN; fi; \
	if [ "$$SIGNER_BOT_BASE_URL_ORIGIN" != "file" ] && [ -n "$$SIGNER_BOT_BASE_URL_ENV" ]; then SIGNER_BOT_BASE_URL="$$SIGNER_BOT_BASE_URL_ENV"; export SIGNER_BOT_BASE_URL; fi; \
	if [ "$$SIGNER_BOT_NETWORK_ORIGIN" != "file" ] && [ -n "$$SIGNER_BOT_NETWORK_ENV" ]; then SIGNER_BOT_NETWORK="$$SIGNER_BOT_NETWORK_ENV"; export SIGNER_BOT_NETWORK; fi; \
	if [ "$$E2E_DOTLI_SMOKE" != "1" ]; then test -n "$$SIGNER_BOT_SVC_TOKEN" || (echo "Missing SIGNER_BOT_SVC_TOKEN. e2e-dotli requires signer-bot; without it a human phone scan is required."; exit 1); fi; \
	$(MAKE) dev-bootstrap; \
	cd $(PLAYGROUND) && bun tests/e2e/dotli-diagnosis.ts

matrix: ## Regenerate the host compatibility matrix from explorer/diagnosis-reports.
	cd $(EXPLORER) && npm run generate-matrix

explorer: ## Run the explorer dev server standalone at http://localhost:5181.
	cd $(EXPLORER) && npx vite --base / --port 5181
