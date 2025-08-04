# BoldOS

## How to run

- Install qemu
- Install rust with https://rustup.rs/
- `cd kernel; cargo run`

```
$ cargo run
   Compiling kernel v0.1.0 (/home/david/code/boldos/kernel)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.17s
     Running `/home/david/code/boldos/kernel/./scripts/run.sh target/aarch64-none-elf/debug/kernel.elf`
--- BoldOS ---
alloc: Initializing early allocator
 user: Starting usermode
Hello from usermode!
DTB mapped at 0x50200000, hexdump of first 32 bytes:
d00dfeed 00100000 00000040 00001db4 00000030 00000011 00000010 00000000 

Boot args: "placeholder kernel params"
RAM: 0p40000000 (268435456 bytes)
Allocating big buffer using newly discovered memory
10MB Buffer at 0x50400000
bye for now...
[PANIC]: not yet implemented: Syscall::Exit not implemented
location: src/aarch64/usermode.rs:143:13

```

## Who needs Jira when you have a todo list

### Milestone 1: Technically a kernel

- [x] Simple page allocator
- [x] MMU & High-memory setup
- [x] Debugging from the IDE with symbol mapping
- [x] Simple interrupt/exception handling
- [x] Spawn usermode thread from an inline buffer
- [x] Handle a log syscall
- [x] Memory-map the DTB to the init process
- [x] Virtual page allocator
- [x] Parse the DTB
  - [x] Tell kernel about the memory nodes
  - [ ] Tell kernel about devices
- [ ] Thread sleeping + cpu idling (timer interrupts)

### Milestone 2: We're getting somewhere

- [ ] Simple drivers from usermode
  - [ ] Monotonic Time 
  - [ ] QEMU fw_cfg
    - [ ] Kernel commandline
    - [ ] Initrd block device
  - [ ] Virtio
    - [ ] Disk (block device)
    - [ ] Console
    - [ ] Network POC
    - [ ] Framebuffer POC
    - [ ] Input POC
    - [ ] RNG POC
- [ ] Spawn multiple threads
- [ ] IPC
  - [ ] Shared memory
  - [ ] Futex
  - [ ] Shared ring buffer over shm & futex
  - [ ] Objects/Interfaces/Methods

### Milestone 3: Usable for something

- [ ] FPU support
- [ ] VFS server
- [ ] FAT32 RO driver
- [ ] Simple shell
- [ ] Very simple TCP stack

### Milestone 4: Optimism is important

- [ ] Multicore
- [ ] ELF file loader
- [ ] Basic unix primitives emulation (fork, socket, tty, etc.) 
- [ ] Libc implementation
- [ ] Python port

### Milestone 5: lmao

- [ ] USB-HID
- [ ] Basic GUI stack
- [ ] DOOM port
- [ ] Basic core utils (not unixy coreutils!)
- [ ] Test suite

### Milestone 6: Probably not happening

- [ ] More interesting USB peripherals (e.g. Ethernet)
- [ ] Run on real hardware (latest raspberry-pi with a QEMU port)
  - [ ] SDHC driver 
- [ ] Virtio GPU acceleration
- [ ] Virtio Sound
- [ ] PCI + NVME

### Milestone 7: Definitely not happening

- [ ] Wi-Fi and/or Bluetooth
- [ ] Linux syscall-level compatibility
- [ ] Wayland implementation
- [ ] Secure boot / TPM
- [ ] Network booting
- [ ] Users/Permissions
- [ ] Package manager / Public build system
