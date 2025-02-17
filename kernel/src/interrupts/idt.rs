use alloc::sync::Arc;
use lazy_static::lazy_static;
use x86_64::instructions::interrupts;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use crate::constants::idt::{SYSCALL_HANDLER, TIMER_VECTOR};
use crate::events::{current_running_event_info, schedule, EventInfo};
use crate::interrupts::x2apic;
use crate::processes::process::{run_process_ring3, ProcessState, PROCESS_TABLE};
use crate::processes::registers::Registers;
use crate::{prelude::*, push_registers};

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(0);
        }
        idt[TIMER_VECTOR].set_handler_fn(timer_handler);
        idt[SYSCALL_HANDLER]
            .set_handler_fn(syscall_handler)
            .set_privilege_level(x86_64::PrivilegeLevel::Ring3);
        idt
    };
}

pub fn init_idt(_cpu_id: u32) {
    IDT.load();
}

pub fn enable() {
    interrupts::enable();
}

pub fn disable() {
    interrupts::disable();
}

pub fn without_interrupts<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    interrupts::without_interrupts(f)
}

pub fn are_enabled() -> bool {
    interrupts::are_enabled()
}

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

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    serial_println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

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

extern "x86-interrupt" fn timer_handler(stack_frame: InterruptStackFrame) {
    push_registers!();

    let mut regs = unsafe {
        let rsp_after: usize;
        core::arch::asm!(
        "mov {}, rsp",
        out(reg) rsp_after);

        // RSP, RIP, and RFLAGS are saved by the interrupt stack frame
        //num registers 15
        let stack_ptr = (rsp_after as *const u64).byte_offset(-472);
        Registers {
            rax: *stack_ptr.add(14),
            rbx: *stack_ptr.add(13),
            rcx: *stack_ptr.add(12),
            rdx: *stack_ptr.add(11),
            rsi: *stack_ptr.add(10),
            rdi: *stack_ptr.add(9),
            r8: *stack_ptr.add(8),
            r9: *stack_ptr.add(7),
            r10: *stack_ptr.add(6),
            r11: *stack_ptr.add(5),
            r12: *stack_ptr.add(4),
            r13: *stack_ptr.add(3),
            r14: *stack_ptr.add(2),
            r15: *stack_ptr.add(1),
            rbp: *stack_ptr.add(0),
            // saved from interrupt stack frame
            rsp: 0,
            rip: 0,
            rflags: 0,
        }
    };
    let cpuid: u32 = x2apic::current_core_id() as u32;
    let event: EventInfo = current_running_event_info(cpuid);
    if event.pid == 0 {
        x2apic::send_eoi();
        return;
    }

    regs.rip = stack_frame.instruction_pointer.as_u64();
    regs.rsp = stack_frame.stack_pointer.as_u64();
    regs.rflags = stack_frame.cpu_flags.bits();

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
            "stc",          // Use carry flag as sentinel to run_process that we're pre-empting
            "ret",
            in(reg) preemption_info.0,
            in(reg) preemption_info.1
        );
    }
}

extern "x86-interrupt" fn syscall_handler(_: InterruptStackFrame) {
    unsafe {
        // I believe we need to save registers
        core::arch::asm!(
            "push rax",
            "call dispatch_syscall",
            "pop rax",
            "iretq",
            options(noreturn)
        )
    }
}
