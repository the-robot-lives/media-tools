INSTALL_DIR ?= $(HOME)/.local/bin
LIB_INSTALL_DIR ?= $(HOME)/.local/lib/media-tools

.PHONY: compile test install clean

compile:
	@python3 -c "import yaml" 2>/dev/null || echo "⚠️  pyyaml not installed: pip3 install pyyaml"
	@python3 -m py_compile lib/media-prompt-engine.py && echo "✓ media-prompt-engine.py compiles"

test:
	@python3 -m py_compile lib/media-prompt-engine.py && echo "✓ Compile check passed"

install:
	@mkdir -p $(INSTALL_DIR) $(LIB_INSTALL_DIR)
	@install -m 755 bin/generate-media-prompt $(INSTALL_DIR)/generate-media-prompt
	@install -m 644 lib/media-prompt-engine.py $(LIB_INSTALL_DIR)/media-prompt-engine.py
	@echo "✓ Installed generate-media-prompt"

clean:
	@rm -f $(INSTALL_DIR)/generate-media-prompt
	@rm -rf $(LIB_INSTALL_DIR)
