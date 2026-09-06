# add-symbol-file kernel/target/aarch64-none-elf/debug/kernel.elf 0x40100000
add-symbol-file kernel/target/aarch64-none-elf/debug/kernel.elf 0xffffff0040100000
add-symbol-file init/target/aarch64-none-elf/release/init.elf 0x10000000

# Break on the second instruction so RustRover actually stops before booting
b *0x40100004
# Break on the first instruction of init.elf
# hbreak *0x10000000

# Skip the commented-out loop in init.s
# thread 1
# set $pc = 0x40100004

