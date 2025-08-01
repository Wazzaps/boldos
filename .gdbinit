# add-symbol-file kernel/target/aarch64-none-elf/debug/kernel.elf 0x40100000
add-symbol-file kernel/target/aarch64-none-elf/debug/kernel.elf 0xffffff0040100000

# Break on the second instruction so RustRover actually stops before booting
b *0x40100004

# Skip the commented-out loop in init.s
# thread 1
# set $pc = 0x40100004

