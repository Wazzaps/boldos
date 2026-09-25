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
[10] init: Hello from usermode!
[15] init: DTB mapped at 0x50200000, hexdump of first 32 bytes:
[16]d00dfeed 00100000 00000040 00001db4 00000030 00000011 00000010 00000000 
[16]
[17] init: Boot args: "placeholder kernel params"
[17] init: RAM: 0p40000000 (268435456 bytes)
[18] init: Expanding heap by 1048576 bytes
[23] init: Mapped memory at 0x50400000
[23] init: Heap vector: [1, 2, 3]
[23] init: Extracting timer information from DTB
 user: LoadKernelDevice: GicAndTimer {
    gicd_base: 0x8000000,
    gicc_base: 0x8010000,
    timer_ppi_interrupt: 0x1e,
    _padding: 0x0,
}
  drv: Initializing ARM GIC
[27] init: ipc_test: Starting
 user: Allocating port for thread 1 at address 0xffffff0040141000 with recv_handle 1
[28] init: ipc_test: Port created: Handle(1), Handle(2)
 user: Creating thread 2
[30] init: ipc_test: Thread created: 2
[30] init: ipc_test: Port empty
 user: cpu idling for: 499ms
[530] init: ipc_test: Port empty
 user: cpu idling for: 500ms
[1031] init: ipc_test: Port empty
 user: cpu idling for: 99ms
[1131] init: ipc_test: Send went OK
 user: cpu idling for: 399ms
[1532] init: ipc_test: Received 15 bytes: 'Hello, world! 0' + 0 handles
 user: cpu idling for: 500ms
[2033] init: ipc_test: Port empty
 user: cpu idling for: 333ms
[2367] init: ipc_test: Send went OK
 user: cpu idling for: 166ms
[2533] init: ipc_test: Received 15 bytes: 'Hello, world! 1' + 0 handles
 user: cpu idling for: 500ms
[3034] init: ipc_test: Port empty
 user: cpu idling for: 500ms
[3535] init: ipc_test: Port empty
 user: cpu idling for: 66ms
[3601] init: ipc_test: Send went OK
 user: cpu idling for: 433ms
[4035] init: ipc_test: Received 15 bytes: 'Hello, world! 2' + 0 handles
 user: cpu idling for: 500ms

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
  - [x] QEMU fw_cfg
    - [x] Initrd blob
  - [ ] Virtio (based on virtio-drivers crate)
    - [x] Disk (block device)
    - [x] 9P filesystem
    - [ ] Console
    - [ ] Network POC
    - [ ] Framebuffer POC
    - [ ] Input POC
    - [ ] RNG POC
- [x] Spawn multiple threads
- [x] Preemptive scheduling (Round-Robin)
- [x] Shared memory with shared page tables (i.e. threads sharing a memory space)
- [x] Basic growable heap allocator in usermode
- [ ] IPC
  - [ ] Shared memory
  - [x] Futex
    - [x] Wait and Wake
    - [x] Timeouts
  - [x] Shared handle tables between threads
  - [ ] Ports (short messages + handles)
    - [x] Passing byte buffers
    - [ ] Passing handles
    - [ ] Buffer pool
  - [ ] Regions (contiguous memory blocks)
  - [ ] MemoryMappings (address spaces for processes)
  - [ ] Waiters (stateful futex/port waiting)
  - [ ] Shared ring buffer over regions & futex
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
