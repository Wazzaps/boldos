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
 user: Creating thread 1
 init: Hello from usermode!
 init: DTB mapped at 0x50200000, hexdump of first 32 bytes:
d00dfeed 00100000 00000040 00001db4 00000030 00000011 00000010 00000000 

 init: Boot args: "placeholder kernel params"
 init: RAM: 0p40000000 (268435456 bytes)
 init: Extracting timer information from DTB
 user: LoadKernelDevice: GicAndTimer {
    gicd_base: 0x8000000,
    gicc_base: 0x8010000,
    timer_ppi_interrupt: 0x1e,
    _padding: 0x0,
}
  drv: Initializing ARM GIC
 init: Creating thread
 user: Creating thread 2
 init: Thread created with ID: 2
 init: Current time: 12 ms
 init: Hello from thread! My PID is 2
 init: Thread Current time: 13 ms
 user: cpu idling for: 999ms
 init: Current time: 1013 ms
 init: Thread Current time: 1013 ms
 user: cpu idling for: 1000ms
 init: Current time: 2015 ms
 init: Thread Current time: 2015 ms
 user: cpu idling for: 1000ms

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
  - [x] Tell kernel about devices
- [x] Thread sleeping + cpu idling (timer interrupts)

### Milestone 2: We're getting somewhere

- [x] Simple drivers from kernelmode
  - [x] ARM GIC
  - [x] ARM Arch Timer
- [ ] Simple drivers from usermode
  - [x] Monotonic Time 
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
- [x] Spawn multiple threads
- [x] Preemptive scheduling (Round-Robin)
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
