use alloc::sync::Arc;
use lazy_static::lazy_static;
use x86_64::instructions::interrupts;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use crate::constants::idt::TIMER_VECTOR;
use crate::events::{current_running_event_info, schedule, EventInfo};
use crate::interrupts::x2apic;
use crate::processes::process::{resume_process, PROCESS_TABLE};
use crate::processes::registers::Registers;
use crate::{prelude::*, push_registers, pop_registers};

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
    // SAVE REGISTERS VALUES FIRST
    push_registers!();
    let mut regs = Registers::new();
    pop_registers!(regs);
    regs.rax = stack_frame.stack_pointer.as_u64();
    regs.rip = stack_frame.instruction_pointer.as_u64();
    regs.rflags = stack_frame.cpu_flags.bits();

    // Get PID from Event from CPU ID
    let cpuid: u32 = x2apic::current_core_id() as u32;
    let event: EventInfo = current_running_event_info(cpuid);

    if event.pid == 0 {
        x2apic::send_eoi();
        return;
    } else if event.pid == 1 {
        serial_println!("Current {:?}", regs);
        panic!();
    }

    serial_println!("Interrupt process {} on {}", event.pid, cpuid);

    // // Get PCB from PID
    let mut process_table = PROCESS_TABLE.write();
    serial_println!("Interrupt process {}", event.pid);

    let process = process_table
        .get_mut(&event.pid)
        .expect("Process not found");
    let mut pcb = process.write();

    // save to the PCB
    pcb.registers = Arc::new(regs);

    // Restore kernel RSP + PC -> RIP from where it stored in run/resume process
    schedule(cpuid, resume_process(event.pid), event.priority, event.pid);
    x2apic::send_eoi();
}
