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
alloc: Expanding kernel heap by 16384 bytes
 user: Creating thread 1
 user: Entering thread 0xffffff0040124808
[6] init: Hello from usermode!
[10] init: DTB mapped at 0x50200000, hexdump of first 32 bytes:
[11]d00dfeed 00100000 00000040 00001db4 00000030 00000011 00000010 00000000 
[11]
[12] init: Boot args: "placeholder kernel params"
[12] init: RAM: 0p40000000 (268435456 bytes)
[12] kernel_api: Expanding heap by 1048576 bytes
[16] kernel_api: Mapped memory at 0x50400000
[16] init: Heap vector: [1, 2, 3]
[16] init: Extracting timer information from DTB
 user: LoadKernelDevice: GicAndTimer {
    gicd_base: 0x8000000,
    gicc_base: 0x8010000,
    timer_ppi_interrupt: 0x1e,
    _padding: 0x0,
}
  drv: Initializing ARM GIC
[28] init: Detected virtio MMIO device with vendor id 0x554D4551, device type Block, version Modern
[29] init: virtio: dma_alloc: phy_addr: 0x40030000 | virt_addr: 0x54400000
(virtio spam truncated...)
 user: Allocating port for thread 1 with recv_handle 4
 user: Creating thread 2
[36] init: Process test: Thread created: 2
[37] kernel_api: Expanding heap by 1048576 bytes
[40] kernel_api: Mapped memory at 0x50400000
[41] example_bin: Hello from example_bin! Here's a heap vector: [1, 2, 3]
[41] example_bin: Received message from parent: Hello from parent!
[42] example_bin: Shmem value: 123
 user: Destroying thread 2 with code 0
 user: cpu idling for: 9998994ms

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
- [x] Refcounted page tables
- [x] Basic growable heap allocator in usermode
- [x] Basic growable heap allocator in kernelmode
- [ ] IPC
  - [x] Shared memory
  - [x] Futex
    - [x] Wait and Wake
    - [x] Timeouts
  - [x] Shared handle tables between threads
  - [ ] Ports (short messages + handles)
    - [x] Passing byte buffers
    - [x] Passing handles
    - [ ] Buffer pool
  - [x] Regions (contiguous memory blocks)
  - [ ] MemoryMappings (address spaces for processes)
    - [x] Mapping of regions to the current address space
    - [x] Unmapping of regions from the current address space
    - [x] Actually reducing the ref count per freed page so the region can be freed
    - [x] Mapping of regions to other address spaces
    - [x] Unmapping of regions from other address spaces
  - [ ] Waiters (stateful futex/port waiting)
  - [ ] Shared ring buffer over regions & futex
  - [ ] Objects/Interfaces/Methods
- [x] Thread/Process termination
- [x] Flat executable loader

### Milestone 3: Usable for something

- [ ] FPU support
- [ ] VFS server
- [ ] FAT32 RO driver
- [ ] Simple shell
- [ ] Very simple TCP stack

### Milestone 4: Optimism is important

- [ ] Multicore
- [x] ELF file loader
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
