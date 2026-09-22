# IPC Design

## The primitives

- Port
  - One-way unbuffered MPSC datagram channel
  - Can transmit short buffers (up to 64KB) and handles (up to 64 per call)
  - Reply port can be given among the handles
  - Can be Recv, Send, or Send-once. Recv right is unique in the system
- Region
  - A contiguous block of virtual memory
  - Can be backed by physical memory, usermode pager, etc.
- MemoryMapping (MM)
  - A list of Regions and their virtual addresses and permissions
- Waiter
  - An object that allows waiting on ports and futexes

## Syscalls

```c
// -- Handles --

typedef uint64_t bo_handle_t;

SYS_RESULT(bo_handle_t) handle_duplicate(bo_handle_t handle);

SYS_RESULT(void) handle_close(bo_handle_t handle);

// -- Ports --

typedef uint32_t bo_port_recv_flags_t;
typedef uint32_t bo_port_send_flags_t;

struct {
  SYS_RESULT(bo_handle_t) recv;
  bo_handle_t send;
} port_create();

struct {
  SYS_RESULT(size_t) num_bytes_received;
  size_t num_handles_received;
} port_recv(
  bo_handle_t port,
  bo_port_recv_flags_t flags,
  void* bytes,
  size_t num_bytes,
  bo_handle_t* handles,
  size_t num_handles
);

SYS_RESULT(void) port_send(
  bo_handle_t port,
  bo_port_send_flags_t flags,
  const void* bytes,
  size_t num_bytes,
  const bo_handle_t* handles,
  size_t num_handles
);

// -- Regions --

typedef uint32_t bo_region_create_virtual_flags_t;
typedef uint32_t bo_region_create_physical_flags_t;

SYS_RESULT(bo_handle_t) region_create_virtual(
  bo_region_create_virtual_flags_t flags,
  size_t num_bytes
);

SYS_RESULT(bo_handle_t) region_create_physical(
  bo_region_create_physical_flags_t flags,
  size_t phy_addr,
  size_t num_bytes,
);

SYS_RESULT(size_t) region_get_size(
  bo_handle_t region
);

SYS_RESULT(size_t) region_read(
  bo_handle_t region,
  void* bytes,
  size_t num_bytes,
  size_t region_offset
);

SYS_RESULT(size_t) region_write(
  bo_handle_t region,
  const void* bytes,
  size_t num_bytes,
  size_t region_offset
);

// -- MemoryMappings --

typedef uint32_t bo_region_arg_flags_t;
typedef uint32_t bo_mm_create_flags_t;

struct bo_region_arg {
  // Specify -1 for unmap and protect operations
  bo_handle_t region;
  size_t offset;
  size_t size;
  bo_region_arg_flags_t flags;
};

SYS_RESULT(bo_handle_t) mm_create(
  bo_mm_create_flags_t flags,
  bo_region_arg* regions,
  size_t num_regions
);

SYS_RESULT(void) mm_modify(
  bo_handle_t mm,
  bo_region_arg* regions,
  size_t num_regions,
);

// -- Waiters --

typedef uint32_t bo_waiter_create_flags_t;
typedef uint32_t bo_waiter_change_flags_t;
typedef uint32_t bo_waiter_event_flags_t;

struct bo_waiter_change {
  bo_waiter_change_flags_t wait_flags;
  uint32_t futex_value;
  size_t user_data;
  union {
    bo_handle_t port;
    void* futex_addr;
  };
};

struct bo_waiter_event {
  bo_waiter_event_flags_t flags;
  /* uint32_t _pad; on 64-bit platforms */
  size_t user_data;
  union {
    bo_handle_t port;
    void* futex_addr;
  };
};

SYS_RESULT(bo_handle_t) waiter_create(
  bo_waiter_create_flags_t flags
);

SYS_RESULT(size_t) waiter_wait(
  bo_handle_t waiter,
  const struct bo_waiter_change* changes,
  size_t num_changes,
  struct bo_waiter_event* events,
  size_t num_events,
  uint64_t timeout_us,
);
```