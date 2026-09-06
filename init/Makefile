.PHONY: all
all: ${OUT_DIR}/init.bin
SRC := $(shell find src -type f)

target/aarch64-none-elf/release/init.elf: $(SRC) Cargo.toml Cargo.lock Makefile
	cargo build --release

${OUT_DIR}/init.bin: target/aarch64-none-elf/release/init.elf Makefile
	llvm-objcopy -O binary -j .text -j .rodata -j .data $< $@
