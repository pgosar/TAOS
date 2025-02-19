use lazy_static::lazy_static;
use x86_64::{
    instructions::interrupts,
    structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode},
};

use alloc::sync::Arc;

use crate::{
    constants::idt::{SYSCALL_HANDLER, TIMER_VECTOR, TLB_SHOOTDOWN_VECTOR},
    events::{current_running_event_info, schedule, EventInfo},
    interrupts::{
        x2apic,
        x2apic::{current_core_id, TLB_SHOOTDOWN_ADDR},
    },
    prelude::*,
    processes::{
        process::{run_process_ring3, ProcessState, PROCESS_TABLE},
        registers::Registers,
    },
};

lazy_static! {
    /// The system's Interrupt Descriptor Table.
    /// Contains handlers for:
    /// - CPU exceptions (breakpoint, page fault, double fault)
    /// - Timer interrupts
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(0);
        }
        idt[TIMER_VECTOR].set_handler_fn(naked_timer_handler);
        idt[SYSCALL_HANDLER]
            .set_handler_fn(syscall_handler)
            .set_privilege_level(x86_64::PrivilegeLevel::Ring3);
        idt[TLB_SHOOTDOWN_VECTOR].set_handler_fn(tlb_shootdown_handler);
        idt
    };
}

/// Loads the IDT for the specified CPU core.
pub fn init_idt(_cpu_id: u32) {
    IDT.load();
}

/// Enables interrupts on the current CPU.
pub fn enable() {
    interrupts::enable();
}

/// Disables interrupts on the current CPU.
pub fn disable() {
    interrupts::disable();
}

/// Executes a closure with interrupts disabled.
///
/// # Arguments
/// * `f` - The closure to execute
///
/// # Returns
/// Returns the result of the closure
pub fn without_interrupts<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    interrupts::without_interrupts(f)
}

/// Checks if interrupts are enabled on the current CPU.
pub fn are_enabled() -> bool {
    interrupts::are_enabled()
}

/// Executes a closure with interrupts enabled, restoring the previous interrupt state after.
///
/// # Arguments
/// * `f` - The closure to execute
///
/// # Returns
/// Returns the result of the closure
pub fn with_interrupts<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    let initially_enabled = are_enabled();
    if !initially_enabled {
        enable();
    }

    let result = f();

    if !initially_enabled {
        disable();
    }

    result
}

/// Handles breakpoint exceptions by printing debug information.
extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    serial_println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

/// Handles double fault exceptions by panicking with debug information.
extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

/// Handles page fault exceptions by printing fault information.
extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    let faulting_address = Cr2::read();

    serial_println!(
        "EXCEPTION: PAGE FAULT\nFaulting Address: {:?}\nError Code: {:X}\n{:#?}",
        faulting_address,
        error_code,
        stack_frame
    );

    panic!("PAGE FAULT!");
}

#[no_mangle]
extern "x86-interrupt" fn syscall_handler(_: InterruptStackFrame) {
    unsafe {
        // I believe we need to save registers
        core::arch::asm!(
            "push rax",
            "call dispatch_syscall",
            "pop rax",
            "iretq",
            options(noreturn)
        );
    }
}

#[naked]
#[allow(undefined_naked_function_abi)]
extern "x86-interrupt" fn naked_timer_handler(_: InterruptStackFrame) {
    unsafe {
        core::arch::naked_asm!(
            "
            push rbp
            push r15
            push r14
            push r13
            push r12
            push r11
            push r10
            push r9
            push r8
            push rdi
            push rsi
            push rdx
            push rcx
            push rbx
            push rax

            cld
            mov	rdi, rsp
            call timer_handler

            pop rax
            pop rbx
            pop rcx
            pop rdx
            pop rsi
            pop rdi
            pop r8
            pop r9
            pop r10
            pop r11
            pop r12
            pop r13
            pop r14
            pop r15
            pop rbp
            iretq
      "
        );
    }
}

#[no_mangle]
fn timer_handler(rsp: u64) {
    let regs = unsafe {
        let stack_ptr: *const u64 = rsp as *const u64;

        Registers {
            rax: *stack_ptr.add(0),
            rbx: *stack_ptr.add(1),
            rcx: *stack_ptr.add(2),
            rdx: *stack_ptr.add(3),
            rsi: *stack_ptr.add(4),
            rdi: *stack_ptr.add(5),
            r8: *stack_ptr.add(6),
            r9: *stack_ptr.add(7),
            r10: *stack_ptr.add(8),
            r11: *stack_ptr.add(9),
            r12: *stack_ptr.add(10),
            r13: *stack_ptr.add(11),
            r14: *stack_ptr.add(12),
            r15: *stack_ptr.add(13),
            rbp: *stack_ptr.add(14),
            // saved from interrupt stack frame
            rsp: *stack_ptr.add(18),
            rip: *stack_ptr.add(15),
            rflags: *stack_ptr.add(17),
        }
    };
    let cpuid: u32 = x2apic::current_core_id() as u32;
    let event: EventInfo = current_running_event_info(cpuid);
    if event.pid == 0 {
        x2apic::send_eoi();
        return;
    }

    // // Get PCB from PID
    let preemption_info = unsafe {
        let mut process_table = PROCESS_TABLE.write();
        let process = process_table
            .get_mut(&event.pid)
            .expect("Process not found");

        let pcb = process.pcb.get();

        // save registers to the PCB
        (*pcb).registers = Arc::new(regs);

        (*pcb).state = ProcessState::Blocked;

        serial_println!("PCB: {:#X?}", *pcb);
        serial_println!("Returning to: {:#x}", (*pcb).kernel_rip);
        ((*pcb).kernel_rsp, (*pcb).kernel_rip)
    };

    unsafe {
        schedule(
            cpuid,
            run_process_ring3(event.pid),
            event.priority,
            event.pid,
        );

        x2apic::send_eoi();

        // Restore kernel RSP + PC -> RIP from where it was stored in run/resume process
        core::arch::asm!(
            "mov rsp, {0}",
            "push {1}",
            "ret",
            in(reg) preemption_info.0,
            in(reg) preemption_info.1
        );
    }
}

// TODO Technically, this design means that when TLB Shootdows happen, each core must sequentially
// invalidate its TLB rather than doing this in parallel. While this is slow, this is of low
// priority to fix
extern "x86-interrupt" fn tlb_shootdown_handler(_: InterruptStackFrame) {
    let core = current_core_id();
    {
        let mut addresses = TLB_SHOOTDOWN_ADDR.lock();
        let vaddr_to_invalidate = addresses[core];
        if vaddr_to_invalidate != 0 {
            unsafe {
                core::arch::asm!("invlpg [{}]", in (reg) vaddr_to_invalidate, options(nostack, preserves_flags));
            }
            addresses[core] = 0;
        }
    }
    x2apic::send_eoi();
}
