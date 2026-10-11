.PHONY: all
all: kernel example_bin

ALL_DEP := Makefile

define crate-sources
	$(shell find $(1)/src -type f) $(1)/Cargo.toml $(1)/Cargo.lock
endef

# --- Build targets ---
.PHONY: kernel
kernel: kernel/target/aarch64-none-elf/release/kernel.elf

kernel/target/aarch64-none-elf/release/kernel.elf: init/target/aarch64-none-elf/release/init.bin $(call crate-sources,kernel) $(ALL_DEP)
	cd kernel && cargo build --release

.PHONY: init
init: init/target/aarch64-none-elf/release/init.bin

init/target/aarch64-none-elf/release/init.bin: $(call crate-sources,init) $(call crate-sources,kernel_api) $(ALL_DEP)
	cd init && cargo build --release
	llvm-objcopy -O binary -j .text -j .rodata -j .data init/target/aarch64-none-elf/release/init.elf $@

.PHONY: example_bin
example_bin: example_bin/target/aarch64-none-elf/release/example-bin.elf

example_bin/target/aarch64-none-elf/release/example-bin.elf: $(call crate-sources,example_bin) $(call crate-sources,kernel_api) $(ALL_DEP)
	cd example_bin && cargo build --release

# --- Run targets ---
.PHONY: run
run: kernel example_bin
	./kernel/scripts/run.sh kernel/target/aarch64-none-elf/release/kernel.elf

.PHONY: debug
debug: kernel example_bin
	./kernel/scripts/run.sh kernel/target/aarch64-none-elf/release/kernel.elf -S

.PHONY: clean
clean:
	cd kernel && cargo clean
	cd example_bin && cargo clean
	cd init && cargo clean
