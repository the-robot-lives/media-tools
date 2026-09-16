INSTALL_DIR ?= $(HOME)/.local/bin
LIB_INSTALL_DIR ?= $(HOME)/.local/lib/media-tools

.PHONY: compile build test test-unit test-structural test-live install clean uninstall install-legacy lab report ffi

compile: build

build:
	@if ! command -v cargo >/dev/null 2>&1; then \
		echo "media-tool: cargo not found; skipping Rust build."; \
	else \
		cargo build --release; \
		echo "✓ Built target/release/generate-media-prompt"; \
	fi

## L0–L2: unit tests + demos dry-run (no paid APIs)
test: test-unit
	@if ! command -v cargo >/dev/null 2>&1; then \
		echo "media-tool: cargo not found; skipping dry-run."; \
	else \
		cargo run -- --dry-run demos/; \
		echo "✓ Unit tests + demos dry-run passed"; \
	fi

test-unit:
	@if ! command -v cargo >/dev/null 2>&1; then \
		echo "media-tool: cargo not found; skipping tests."; \
	else \
		cargo test; \
	fi

## Structural probes on existing demo AV outputs (ffprobe)
test-structural: test-unit
	@./scripts/live-eval-report.sh

## L3 live generate (API cost). Optional: TYPE=image make test-live
test-live: build
	@./scripts/live-eval-report.sh --generate

## Markdown report only (no cargo test)
report:
	@./scripts/live-eval-report.sh

## Interactive web lab (types · prompts · generate · view · eval)
## Usage: make lab   OR   make lab PORT=9090
PORT ?= 8787
lab:
	@if ! command -v cargo >/dev/null 2>&1; then \
		echo "media-tool: cargo not found"; exit 1; \
	fi
	cargo run -- lab --port $(PORT) --verbose

## Regenerate the UniFFI Swift bindings into macos/MediaWorkbench.
## Run this after ANY change to src/ffi.rs; the generated files are committed.
## The generator builds into its own target dir: enabling the `bindgen` feature would
## otherwise re-resolve features for the whole crate and churn target/release/.
BINDGEN_OUT := target/uniffi-swift
BRIDGE_SRC  := macos/MediaWorkbench/Sources/MediaToolBridge
BRIDGE_FFI  := macos/MediaWorkbench/Sources/media_toolFFI
ffi:
	# --lib only: the bindings need libmedia_tool.{a,dylib}, not the LTO'd CLI binary.
	cargo build --release --lib
	CARGO_TARGET_DIR=target/bindgen \
		cargo run --release --features bindgen --bin uniffi-bindgen -- generate \
		--library $(CURDIR)/target/release/libmedia_tool.dylib \
		--language swift --no-format \
		--out-dir $(CURDIR)/$(BINDGEN_OUT)
	@mkdir -p $(BRIDGE_SRC) $(BRIDGE_FFI)
	cp $(BINDGEN_OUT)/media_tool.swift $(BRIDGE_SRC)/media_tool.swift
	cp $(BINDGEN_OUT)/media_toolFFI.h $(BRIDGE_FFI)/media_toolFFI.h
	cp $(BINDGEN_OUT)/media_toolFFI.modulemap $(BRIDGE_FFI)/module.modulemap
	@echo "✓ Regenerated Swift bindings (staticlib: target/release/libmedia_tool.a)"

install: build
	@if ! command -v cargo >/dev/null 2>&1; then \
		echo "media-tool: cargo not found; no Rust binary to install." ; \
		$(MAKE) install-legacy ; \
	else \
		mkdir -p $(INSTALL_DIR); \
		install -m 755 target/release/generate-media-prompt $(INSTALL_DIR)/generate-media-prompt; \
		echo "✓ Installed generate-media-prompt (Rust)"; \
	fi

install-legacy:
	@mkdir -p $(INSTALL_DIR) $(LIB_INSTALL_DIR)
	@install -m 755 bin/generate-media-prompt $(INSTALL_DIR)/generate-media-prompt
	@install -m 644 lib/media-prompt-engine.py $(LIB_INSTALL_DIR)/media-prompt-engine.py
	@echo "✓ Installed generate-media-prompt (Python/bash)"

clean:
	@cargo clean 2>/dev/null || true

uninstall:
	@rm -f $(INSTALL_DIR)/generate-media-prompt
	@rm -rf $(LIB_INSTALL_DIR)
