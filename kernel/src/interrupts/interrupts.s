.text
.global interrupt_stubs

// Macros to define stub code
.macro def_stub_no_err num
stub\num:
    pushq $0                    // Push dummy error code
    pushq $\num                 // Push interrupt number
    jmp interrupt_common
.endm

.macro def_stub_err num
stub\num:
    pushq $\num                 // Push interrupt number
    jmp interrupt_common
.endm

// Common interrupt handling code
interrupt_common:
    pushq %r15
    pushq %r14
    pushq %r13
    pushq %r12
    pushq %r11
    pushq %r10
    pushq %r9
    pushq %r8
    pushq %rbp
    pushq %rdi
    pushq %rsi
    pushq %rdx
    pushq %rcx
    pushq %rbx
    pushq %rax

    movq %rsp, %rdi             // First argument is stack frame
    call common_interrupt_handler

    // Restore registers (reverse order of saves)
    popq %rax
    popq %rbx
    popq %rcx
    popq %rdx
    popq %rsi
    popq %rdi
    popq %rbp
    popq %r8
    popq %r9
    popq %r10
    popq %r11
    popq %r12
    popq %r13
    popq %r14
    popq %r15

    addq $16, %rsp              // Pop error code and interrupt number
    iretq

// Array of interrupt stubs
.align 8
interrupt_stubs:
    .quad stub0   // 0: Divide by zero
    .quad stub1   // 1: Debug
    .quad stub2   // 2: NMI
    .quad stub3   // 3: Breakpoint
    .quad stub4   // 4: Overflow
    .quad stub5   // 5: Bound range exceeded
    .quad stub6   // 6: Invalid opcode
    .quad stub7   // 7: Device not available
    .quad stub8   // 8: Double fault
    .quad stub9   // 9: Coprocessor segment overrun
    .quad stub10  // 10: Invalid TSS
    .quad stub11  // 11: Segment not present
    .quad stub12  // 12: Stack-segment fault
    .quad stub13  // 13: General protection fault
    .quad stub14  // 14: Page fault
    .quad stub15  // 15: Reserved
    .quad stub16  // 16: x87 FPU error
    .quad stub17  // 17: Alignment check
    .quad stub18  // 18: Machine check
    .quad stub19  // 19: SIMD floating point
    .quad stub20  // 20: Virtualization
    .quad stub21  // 21: Control protection
    .quad stub22  // 22: Reserved
    .quad stub23  // 23: Reserved
    .quad stub24  // 24: Reserved
    .quad stub25  // 25: Reserved
    .quad stub26  // 26: Reserved
    .quad stub27  // 27: Reserved
    .quad stub28  // 28: Reserved
    .quad stub29  // 29: Reserved
    .quad stub30  // 30: Security exception
    .quad stub31  // 31: Reserved

// Define stubs
.align 8
def_stub_no_err 0
def_stub_no_err 1
def_stub_no_err 2
def_stub_no_err 3
def_stub_no_err 4
def_stub_no_err 5
def_stub_no_err 6
def_stub_no_err 7
def_stub_err 8
def_stub_no_err 9
def_stub_err 10
def_stub_err 11
def_stub_err 12
def_stub_err 13
def_stub_err 14
def_stub_no_err 15
def_stub_no_err 16
def_stub_err 17
def_stub_no_err 18
def_stub_no_err 19
def_stub_no_err 20
def_stub_err 21
def_stub_no_err 22
def_stub_no_err 23
def_stub_no_err 24
def_stub_no_err 25
def_stub_no_err 26
def_stub_no_err 27
def_stub_no_err 28
def_stub_no_err 29
def_stub_err 30
def_stub_no_err 31
