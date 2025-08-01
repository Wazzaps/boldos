.section .text.init, "ax"
.global _start

_start:
// Enable if debugger is misbehaving
// 1:
// b 1b

// Erase bss
ldr x10, =_bss_start_phys
ldr x11, =_bss_end_phys
cmp x10, x11
beq 2f
1:
str xzr, [x10], #8
cmp x10, x11
bne 1b
2:

// Setup reset vector
ldr x10, =_vectors
mov w10, w10  // Truncate to low mem
msr vbar_el1, x10

// Set stack and call main
ldr x10, =_initstack_end_phys
mov sp, x10
ldr x10, =kmain_nommu
mov w10, w10
br x10

// Reset vector
.align 11
.global _vectors
_vectors:
// synchronous
.align  7
// Make room on the stack for the exception context.
sub     sp, sp, #16 * 17
// Store all general purpose registers on the stack.
stp     x0, x1, [sp, #16 * 0]
stp     x2, x3, [sp, #16 * 1]
stp     x4, x5, [sp, #16 * 2]
stp     x6, x7, [sp, #16 * 3]
stp     x8, x9, [sp, #16 * 4]
stp     x10, x11, [sp, #16 * 5]
stp     x12, x13, [sp, #16 * 6]
stp     x14, x15, [sp, #16 * 7]
stp     x16, x17, [sp, #16 * 8]
stp     x18, x19, [sp, #16 * 9]
stp     x20, x21, [sp, #16 * 10]
stp     x22, x23, [sp, #16 * 11]
stp     x24, x25, [sp, #16 * 12]
stp     x26, x27, [sp, #16 * 13]
stp     x28, x29, [sp, #16 * 14]
// Add the exception link register (ELR_EL1) and the saved program status (SPSR_EL1).
mrs     x1, ELR_EL1
stp     lr, x1, [sp, #16 * 15]
mrs     x1, SPSR_EL1
mrs     x2, SP_EL0
stp     x1, x2, [sp, #16 * 16]
// x0 is the first argument for the function called through `handler`.
mov     x0, sp
bl      exception_handler2
b       __exception_restore_context

// IRQ
.align  7
// Make room on the stack for the exception context.
sub     sp, sp, #16 * 17
// Store all general purpose registers on the stack.
stp     x0, x1, [sp, #16 * 0]
stp     x2, x3, [sp, #16 * 1]
stp     x4, x5, [sp, #16 * 2]
stp     x6, x7, [sp, #16 * 3]
stp     x8, x9, [sp, #16 * 4]
stp     x10, x11, [sp, #16 * 5]
stp     x12, x13, [sp, #16 * 6]
stp     x14, x15, [sp, #16 * 7]
stp     x16, x17, [sp, #16 * 8]
stp     x18, x19, [sp, #16 * 9]
stp     x20, x21, [sp, #16 * 10]
stp     x22, x23, [sp, #16 * 11]
stp     x24, x25, [sp, #16 * 12]
stp     x26, x27, [sp, #16 * 13]
stp     x28, x29, [sp, #16 * 14]
// Add the exception link register (ELR_EL1) and the saved program status (SPSR_EL1).
mrs     x1, ELR_EL1
stp     lr, x1, [sp, #16 * 15]
mrs     x1, SPSR_EL1
mrs     x2, SP_EL0
stp     x1, x2, [sp, #16 * 16]
// x0 is the first argument for the function called through `handler`.
mov     x0, sp
bl      irq_handler
b       __exception_restore_context

// FIQ
.align  7
mov     x0, #2
mrs     x1, esr_el1
mrs     x2, elr_el1
mrs     x3, spsr_el1
mrs     x4, far_el1
b       exception_handler

// SError
.align  7
mov     x0, #3
mrs     x1, esr_el1
mrs     x2, elr_el1
mrs     x3, spsr_el1
mrs     x4, far_el1
b       exception_handler
// synchronous
.align  7
// Make room on the stack for the exception context.
sub     sp, sp, #16 * 17
// Store all general purpose registers on the stack.
stp     x0, x1, [sp, #16 * 0]
stp     x2, x3, [sp, #16 * 1]
stp     x4, x5, [sp, #16 * 2]
stp     x6, x7, [sp, #16 * 3]
stp     x8, x9, [sp, #16 * 4]
stp     x10, x11, [sp, #16 * 5]
stp     x12, x13, [sp, #16 * 6]
stp     x14, x15, [sp, #16 * 7]
stp     x16, x17, [sp, #16 * 8]
stp     x18, x19, [sp, #16 * 9]
stp     x20, x21, [sp, #16 * 10]
stp     x22, x23, [sp, #16 * 11]
stp     x24, x25, [sp, #16 * 12]
stp     x26, x27, [sp, #16 * 13]
stp     x28, x29, [sp, #16 * 14]
// Add the exception link register (ELR_EL1) and the saved program status (SPSR_EL1).
mrs     x1, ELR_EL1
stp     lr, x1, [sp, #16 * 15]
mrs     x1, SPSR_EL1
mrs     x2, SP_EL0
stp     x1, x2, [sp, #16 * 16]
// x0 is the first argument for the function called through `handler`.
mov     x0, sp
bl      exception_handler2
b       __exception_restore_context

// IRQ
.align  7
// Make room on the stack for the exception context.
sub     sp, sp, #16 * 17
// Store all general purpose registers on the stack.
stp     x0, x1, [sp, #16 * 0]
stp     x2, x3, [sp, #16 * 1]
stp     x4, x5, [sp, #16 * 2]
stp     x6, x7, [sp, #16 * 3]
stp     x8, x9, [sp, #16 * 4]
stp     x10, x11, [sp, #16 * 5]
stp     x12, x13, [sp, #16 * 6]
stp     x14, x15, [sp, #16 * 7]
stp     x16, x17, [sp, #16 * 8]
stp     x18, x19, [sp, #16 * 9]
stp     x20, x21, [sp, #16 * 10]
stp     x22, x23, [sp, #16 * 11]
stp     x24, x25, [sp, #16 * 12]
stp     x26, x27, [sp, #16 * 13]
stp     x28, x29, [sp, #16 * 14]
// Add the exception link register (ELR_EL1) and the saved program status (SPSR_EL1).
mrs     x1, ELR_EL1
stp     lr, x1, [sp, #16 * 15]
mrs     x1, SPSR_EL1
mrs     x2, SP_EL0
stp     x1, x2, [sp, #16 * 16]
// x0 is the first argument for the function called through `handler`.
mov     x0, sp
bl      irq_handler
b       __exception_restore_context

// FIQ
.align  7
mov     x0, #2
mrs     x1, esr_el1
mrs     x2, elr_el1
mrs     x3, spsr_el1
mrs     x4, far_el1
b       exception_handler

// SError
.align  7
mov     x0, #3
mrs     x1, esr_el1
mrs     x2, elr_el1
mrs     x3, spsr_el1
mrs     x4, far_el1
b       exception_handler
// synchronous
.align  7
// Make room on the stack for the exception context.
sub     sp, sp, #16 * 17
// Store all general purpose registers on the stack.
stp     x0, x1, [sp, #16 * 0]
stp     x2, x3, [sp, #16 * 1]
stp     x4, x5, [sp, #16 * 2]
stp     x6, x7, [sp, #16 * 3]
stp     x8, x9, [sp, #16 * 4]
stp     x10, x11, [sp, #16 * 5]
stp     x12, x13, [sp, #16 * 6]
stp     x14, x15, [sp, #16 * 7]
stp     x16, x17, [sp, #16 * 8]
stp     x18, x19, [sp, #16 * 9]
stp     x20, x21, [sp, #16 * 10]
stp     x22, x23, [sp, #16 * 11]
stp     x24, x25, [sp, #16 * 12]
stp     x26, x27, [sp, #16 * 13]
stp     x28, x29, [sp, #16 * 14]
// Add the exception link register (ELR_EL1) and the saved program status (SPSR_EL1).
mrs     x1, ELR_EL1
stp     lr, x1, [sp, #16 * 15]
mrs     x1, SPSR_EL1
mrs     x2, SP_EL0
stp     x1, x2, [sp, #16 * 16]
// x0 is the first argument for the function called through `handler`.
mov     x0, sp
bl      exception_handler2
b       __exception_restore_context

// IRQ
.align  7
// Make room on the stack for the exception context.
sub     sp, sp, #16 * 17
// Store all general purpose registers on the stack.
stp     x0, x1, [sp, #16 * 0]
stp     x2, x3, [sp, #16 * 1]
stp     x4, x5, [sp, #16 * 2]
stp     x6, x7, [sp, #16 * 3]
stp     x8, x9, [sp, #16 * 4]
stp     x10, x11, [sp, #16 * 5]
stp     x12, x13, [sp, #16 * 6]
stp     x14, x15, [sp, #16 * 7]
stp     x16, x17, [sp, #16 * 8]
stp     x18, x19, [sp, #16 * 9]
stp     x20, x21, [sp, #16 * 10]
stp     x22, x23, [sp, #16 * 11]
stp     x24, x25, [sp, #16 * 12]
stp     x26, x27, [sp, #16 * 13]
stp     x28, x29, [sp, #16 * 14]
// Add the exception link register (ELR_EL1) and the saved program status (SPSR_EL1).
mrs     x1, ELR_EL1
stp     lr, x1, [sp, #16 * 15]
mrs     x1, SPSR_EL1
mrs     x2, SP_EL0
stp     x1, x2, [sp, #16 * 16]
// x0 is the first argument for the function called through `handler`.
mov     x0, sp
bl      irq_handler
b       __exception_restore_context

// FIQ
.align  7
mov     x0, #2
mrs     x1, esr_el1
mrs     x2, elr_el1
mrs     x3, spsr_el1
mrs     x4, far_el1
b       exception_handler

// SError
.align  7
mov     x0, #3
mrs     x1, esr_el1
mrs     x2, elr_el1
mrs     x3, spsr_el1
mrs     x4, far_el1
b       exception_handler
// synchronous
.align  7
// Make room on the stack for the exception context.
sub     sp, sp, #16 * 17
// Store all general purpose registers on the stack.
stp     x0, x1, [sp, #16 * 0]
stp     x2, x3, [sp, #16 * 1]
stp     x4, x5, [sp, #16 * 2]
stp     x6, x7, [sp, #16 * 3]
stp     x8, x9, [sp, #16 * 4]
stp     x10, x11, [sp, #16 * 5]
stp     x12, x13, [sp, #16 * 6]
stp     x14, x15, [sp, #16 * 7]
stp     x16, x17, [sp, #16 * 8]
stp     x18, x19, [sp, #16 * 9]
stp     x20, x21, [sp, #16 * 10]
stp     x22, x23, [sp, #16 * 11]
stp     x24, x25, [sp, #16 * 12]
stp     x26, x27, [sp, #16 * 13]
stp     x28, x29, [sp, #16 * 14]
// Add the exception link register (ELR_EL1) and the saved program status (SPSR_EL1).
mrs     x1, ELR_EL1
stp     lr, x1, [sp, #16 * 15]
mrs     x1, SPSR_EL1
mrs     x2, SP_EL0
stp     x1, x2, [sp, #16 * 16]
// x0 is the first argument for the function called through `handler`.
mov     x0, sp
bl      exception_handler2
b       __exception_restore_context

// IRQ
.align  7
// Make room on the stack for the exception context.
sub     sp, sp, #16 * 17
// Store all general purpose registers on the stack.
stp     x0, x1, [sp, #16 * 0]
stp     x2, x3, [sp, #16 * 1]
stp     x4, x5, [sp, #16 * 2]
stp     x6, x7, [sp, #16 * 3]
stp     x8, x9, [sp, #16 * 4]
stp     x10, x11, [sp, #16 * 5]
stp     x12, x13, [sp, #16 * 6]
stp     x14, x15, [sp, #16 * 7]
stp     x16, x17, [sp, #16 * 8]
stp     x18, x19, [sp, #16 * 9]
stp     x20, x21, [sp, #16 * 10]
stp     x22, x23, [sp, #16 * 11]
stp     x24, x25, [sp, #16 * 12]
stp     x26, x27, [sp, #16 * 13]
stp     x28, x29, [sp, #16 * 14]
// Add the exception link register (ELR_EL1) and the saved program status (SPSR_EL1).
mrs     x1, ELR_EL1
stp     lr, x1, [sp, #16 * 15]
mrs     x1, SPSR_EL1
mrs     x2, SP_EL0
stp     x1, x2, [sp, #16 * 16]
// x0 is the first argument for the function called through `handler`.
mov     x0, sp
bl      irq_handler
b       __exception_restore_context

// FIQ
.align  7
mov     x0, #2
mrs     x1, esr_el1
mrs     x2, elr_el1
mrs     x3, spsr_el1
mrs     x4, far_el1
b       exception_handler

// SError
.align  7
mov     x0, #3
mrs     x1, esr_el1
mrs     x2, elr_el1
mrs     x3, spsr_el1
mrs     x4, far_el1
b       exception_handler

// Jump back
__exception_restore_context:
ldp x19, x20, [sp, #16 * 16]
msr SPSR_EL1, x19
msr SP_EL0,   x20
ldp lr,  x20, [sp, #16 * 15]
msr ELR_EL1,  x20
ldp x0,  x1,  [sp, #16 * 0]
ldp x2,  x3,  [sp, #16 * 1]
ldp x4,  x5,  [sp, #16 * 2]
ldp x6,  x7,  [sp, #16 * 3]
ldp x8,  x9,  [sp, #16 * 4]
ldp x10, x11, [sp, #16 * 5]
ldp x12, x13, [sp, #16 * 6]
ldp x14, x15, [sp, #16 * 7]
ldp x16, x17, [sp, #16 * 8]
ldp x18, x19, [sp, #16 * 9]
ldp x20, x21, [sp, #16 * 10]
ldp x22, x23, [sp, #16 * 11]
ldp x24, x25, [sp, #16 * 12]
ldp x26, x27, [sp, #16 * 13]
ldp x28, x29, [sp, #16 * 14]
add sp,  sp,  #16 * 17
eret
