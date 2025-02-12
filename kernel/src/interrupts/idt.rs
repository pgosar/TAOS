use alloc::sync::Arc;
use lazy_static::lazy_static;
use x86_64::instructions::interrupts;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use crate::constants::idt::TIMER_VECTOR;
use crate::events::current_running_event_pid;
use crate::interrupts::x2apic;
use crate::prelude::*;
use crate::processes::process::{print_process_table, run_process_ring3, Registers, PROCESS_TABLE};
use spin::rwlock::RwLock;

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

extern "x86-interrupt" fn timer_handler(_: InterruptStackFrame) {
    // SAVE REGISTERS VALUES FIRST
    let rax = unsafe {
        let mut rax: u32 = 129387942;
        core::arch::asm!(
            "mov {}, rax",
            out(reg) rax,
        );

        rax
    };

    // Get event from CPU ID (from event runner)
    let cpuid: u32 = x2apic::current_core_id() as u32;

    if cpuid == 0 {
        serial_println!("{:#X}", rax);
    }

    // Get PID from event
    let pid = current_running_event_pid(cpuid);

    if pid == 0 {
        x2apic::send_eoi();
        return;
    }

    serial_println!("Interrupt process {} on {}", pid, cpuid);

    // // Get PCB from PID
    let mut process_table = PROCESS_TABLE.write();
    serial_println!("Interrupt process {}", pid);

    let process = process_table.get_mut(&pid).expect("Process not found");
    let mut pcb = process.write();

    let regs = unsafe {
        let regs = Arc::make_mut(&mut pcb.registers);
        serial_println!("Current registers are {:#X?}", regs);
        core::arch::asm!(
            "mov {}, rax",
            out(reg) regs.rax,
        );
        serial_println!("The RAX value is {:#X}", regs.rax);

        regs
    };


    // // Choose next process to run
    // let next_pid = schedule_next_process();

    // // Run the next process
    // let next_proc = process_table
    //     .get(&next_pid)
    //     .expect("Next process not found");
    // unsafe {
    //     run_process_ring3(next_pid, next_proc.registers);
    // }

    x2apic::send_eoi();
}
