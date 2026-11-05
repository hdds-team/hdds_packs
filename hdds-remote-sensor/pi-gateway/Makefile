# HDDS Pi Gateway - Cross-compilation Makefile
# Uses cargo-zigbuild for targeting older glibc versions (Pi Zero 2 W has glibc 2.36)

TARGET := aarch64-unknown-linux-gnu
GLIBC_VERSION := 2.36
BINARY := hdds-pi-gateway
PI_HOST ?= pi@raspberrypi.local
PI_PATH ?= ~/$(BINARY)

.PHONY: all build release deploy clean help

all: release

# Debug build
build:
	cargo zigbuild --target $(TARGET).$(GLIBC_VERSION)

# Release build (optimized, stripped)
release:
	cargo zigbuild --release --target $(TARGET).$(GLIBC_VERSION)

# Deploy to Raspberry Pi
deploy: release
	@echo "Stopping existing process on Pi..."
	-ssh $(PI_HOST) "pkill $(BINARY)" 2>/dev/null || true
	@sleep 1
	@echo "Deploying to $(PI_HOST):$(PI_PATH)..."
	scp target/$(TARGET)/release/$(BINARY) $(PI_HOST):$(PI_PATH)
	@echo "Done! Run with: ssh $(PI_HOST) 'RUST_LOG=info $(PI_PATH)'"

# Deploy and run
run: deploy
	ssh $(PI_HOST) "RUST_LOG=info $(PI_PATH)"

# Clean build artifacts
clean:
	cargo clean

# Show help
help:
	@echo "HDDS Pi Gateway Makefile"
	@echo ""
	@echo "Targets:"
	@echo "  build    - Debug build for aarch64"
	@echo "  release  - Release build for aarch64 (default)"
	@echo "  deploy   - Build and copy to Pi"
	@echo "  run      - Deploy and run on Pi"
	@echo "  clean    - Remove build artifacts"
	@echo ""
	@echo "Variables:"
	@echo "  PI_HOST  - SSH target (default: $(PI_HOST))"
	@echo "  PI_PATH  - Remote path (default: $(PI_PATH))"
	@echo ""
	@echo "Prerequisites:"
	@echo "  - cargo-zigbuild: cargo install cargo-zigbuild"
	@echo "  - zig compiler: https://ziglang.org/download/"
