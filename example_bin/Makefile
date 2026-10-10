.PHONY: all
all: ${OUT_DIR}/example-bin.bin
SRC := $(shell find src -type f)

target/aarch64-none-elf/release/example-bin.elf: $(SRC) Cargo.toml Cargo.lock Makefile
	cargo build --release

${OUT_DIR}/example-bin.bin: target/aarch64-none-elf/release/example-bin.elf Makefile
	llvm-objcopy -O binary -j .text -j .rodata -j .data $< $@
