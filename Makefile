SHELL := /usr/bin/env bash
.SHELLFLAGS := -euo pipefail -c

TRUAPI_PKG := js/packages/truapi
PLAYGROUND := playground
DOTLI      := hosts/dotli

.PHONY: all help setup build codegen test playground check \
        rust-build rust-fmt rust-clippy rust-test \
        ts-build ts-test \
        playground-snapshot playground-build playground-lint playground-e2e

all: build

help:
	@echo "TrUAPI Makefile targets:"
	@echo ""
	@echo "  setup       First-time setup (submodules, install deps)"
	@echo "  build       Build the Rust workspace and the TS client (default)"
	@echo "  codegen     Regenerate the TS client from the Rust crate"
	@echo "  test        Run Rust tests and TS tests"
	@echo "  playground  Refresh the playground snapshot and build it"
	@echo "  check       Full verification suite (Rust + TS + playground + e2e)"
	@echo ""
	@echo "Atomic targets: rust-build, rust-fmt, rust-clippy, rust-test,"
	@echo "                ts-build, ts-test,"
	@echo "                playground-snapshot, playground-build,"
	@echo "                playground-lint, playground-e2e"

setup:
	git submodule update --init --recursive
	cd $(TRUAPI_PKG) && npm install
	cd $(PLAYGROUND) && yarn install --frozen-lockfile
	cd $(DOTLI) && bun install

build: rust-build ts-build

codegen:
	./scripts/codegen.sh

test: rust-test ts-test

playground: playground-snapshot playground-build

check: rust-build rust-fmt rust-clippy rust-test \
       ts-build ts-test \
       playground-snapshot playground-build playground-lint playground-e2e

rust-build:
	cargo build --workspace

rust-fmt:
	cargo +nightly fmt --check

rust-clippy:
	cargo clippy --workspace --all-targets --all-features -- -D warnings

rust-test:
	cargo test --workspace

ts-build:
	cd $(TRUAPI_PKG) && npm run build

ts-test:
	cd $(TRUAPI_PKG) && npm test

playground-snapshot:
	rm -rf $(PLAYGROUND)/node_modules
	cd $(PLAYGROUND) && yarn install --frozen-lockfile

playground-build:
	cd $(PLAYGROUND) && yarn build

playground-lint:
	cd $(PLAYGROUND) && yarn lint

playground-e2e:
	cd $(PLAYGROUND) && yarn e2e
