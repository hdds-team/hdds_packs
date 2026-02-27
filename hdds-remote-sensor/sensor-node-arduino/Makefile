# Makefile for HDDS Remote Sensor - ESP32 Arduino Project
#
# This Makefile ensures the hdds-micro-c library is built and installed
# before compiling the Arduino project.
#
# Usage:
#   make        - Build library + Arduino project
#   make lib    - Only rebuild hdds-micro-c library
#   make build  - Only build Arduino project
#   make flash  - Upload to ESP32
#   make monitor - Open serial monitor
#   make clean  - Clean all build artifacts
#   make all    - Full rebuild + flash

# Paths
HDDS_ROOT := $(realpath ../../../hdds)
HDDS_MICRO_C := $(HDDS_ROOT)/crates/hdds-micro-c
LIB_DIR := lib/hdds-micro
ESP_TOOLCHAIN := $(HOME)/.rustup/toolchains/esp
LIBCLANG_PATH := $(ESP_TOOLCHAIN)/xtensa-esp32-elf-clang/esp-18.1.2_20240912/esp-clang/lib

# Check if ESP toolchain is available
ESP_AVAILABLE := $(shell test -x "$(ESP_TOOLCHAIN)/bin/cargo" && echo yes || echo no)

.PHONY: all lib build flash monitor clean check-esp help

# Default target: build library and Arduino project
default: lib build

# Full workflow: clean, rebuild lib, rebuild Arduino, flash
all: clean lib build flash

check-esp:
ifeq ($(ESP_AVAILABLE),no)
	@echo "ERROR: ESP Rust toolchain not found at $(ESP_TOOLCHAIN)"
	@echo "Install with: espup install"
	@exit 1
endif

# Build the hdds-micro-c static library for ESP32
lib: check-esp
	@echo "=== Building hdds-micro-c for ESP32 ==="
	cd $(HDDS_ROOT) && \
		RUSTUP_TOOLCHAIN=esp \
		LIBCLANG_PATH=$(LIBCLANG_PATH) \
		cargo build --release --target xtensa-esp32-espidf \
		-Zbuild-std=std,panic_abort \
		-p hdds-micro-c
	@echo "=== Installing library ==="
	@mkdir -p $(LIB_DIR)
	cp $(HDDS_ROOT)/target/xtensa-esp32-espidf/release/libhdds_micro_c.a $(LIB_DIR)/
	@echo "Library updated: $(LIB_DIR)/libhdds_micro_c.a"

# Build Arduino project
build:
	@echo "=== Building Arduino project ==="
	pio run

# Upload to ESP32
flash:
	@echo "=== Flashing ESP32 ==="
	pio run -t upload

# Open serial monitor
monitor:
	pio device monitor -b 115200

# Clean all build artifacts
clean:
	@echo "=== Cleaning ==="
	pio run -t clean
	@echo "Cleaned Arduino build"

# Full clean including library
cleanall: clean
	rm -rf $(HDDS_ROOT)/target/xtensa-esp32-espidf
	@echo "Cleaned library build"

# Show current status
status:
	@echo "HDDS Root: $(HDDS_ROOT)"
	@echo "ESP Toolchain: $(ESP_TOOLCHAIN)"
	@echo "ESP Available: $(ESP_AVAILABLE)"
	@ls -la $(LIB_DIR)/libhdds_micro_c.a 2>/dev/null || echo "Library not built"

# Help
help:
	@echo "HDDS Remote Sensor - ESP32 Arduino Project"
	@echo ""
	@echo "Targets:"
	@echo "  make          Build library + Arduino project"
	@echo "  make all      Full rebuild + flash"
	@echo "  make lib      Only rebuild hdds-micro-c library"
	@echo "  make build    Only build Arduino project"
	@echo "  make flash    Upload to ESP32"
	@echo "  make monitor  Open serial monitor"
	@echo "  make clean    Clean Arduino build"
	@echo "  make cleanall Clean everything including library"
	@echo "  make status   Show current status"
	@echo ""
	@echo "ESP Toolchain: $(ESP_AVAILABLE)"
