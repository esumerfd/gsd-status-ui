BIN_NAME := gsd-status
RELEASE_BIN := target/release/$(BIN_NAME)
INSTALL_DIR := $(HOME)/bin
INSTALL_PATH := $(INSTALL_DIR)/$(BIN_NAME)

.PHONY: all build release debug install uninstall run clean check fmt help

all: release

build release: $(RELEASE_BIN)

$(RELEASE_BIN): $(shell find src -name '*.rs') Cargo.toml
	cargo build --release

debug:
	cargo build

check:
	cargo check

fmt:
	cargo fmt

run: release
	$(RELEASE_BIN)

install: release | $(INSTALL_DIR)
	install -m 0755 $(RELEASE_BIN) $(INSTALL_PATH)
	@echo "installed → $(INSTALL_PATH)"

$(INSTALL_DIR):
	mkdir -p $(INSTALL_DIR)

uninstall:
	rm -f $(INSTALL_PATH)
	@echo "removed → $(INSTALL_PATH)"

clean:
	cargo clean

help:
	@echo "Targets:"
	@echo "  make build / release  — build release binary"
	@echo "  make debug            — build debug binary"
	@echo "  make run              — build + run against \$$PWD"
	@echo "  make install          — copy binary to $(INSTALL_DIR)"
	@echo "  make uninstall        — remove installed binary"
	@echo "  make check / fmt      — cargo check / fmt"
	@echo "  make clean            — cargo clean"
