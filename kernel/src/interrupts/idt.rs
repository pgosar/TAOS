use lazy_static::lazy_static;
use x86_64::instructions::interrupts;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use x86_64::structures::paging::{OffsetPageTable, Page, PageTable};
use x86_64::VirtAddr;

use crate::constants::idt::TIMER_VECTOR;
use crate::interrupts::x2apic;
use crate::prelude::*;
use crate::memory::paging::{create_mapping, HHDM_OFFSET};

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
    use x86_64::registers::control::{Cr2, Cr3};

    let faulting_address = Cr2::read().expect("Cannot read faulting address").as_u64();
    let pml4 = Cr3::read().0;
    let new_pml4_phys = pml4.start_address();
    let new_pml4_virt = VirtAddr::new(*HHDM_OFFSET) + new_pml4_phys.as_u64();
    let new_pml4_ptr: *mut PageTable = new_pml4_virt.as_mut_ptr();

    let mut mapper = unsafe {
        OffsetPageTable::new(&mut *new_pml4_ptr, VirtAddr::new(*HHDM_OFFSET))
    };

    let stack_pointer = stack_frame.stack_pointer.as_u64();

    serial_println!(
        "EXCEPTION: PAGE FAULT\nFaulting Address: {:?}\nError Code: {:X}\n{:#?}",
        faulting_address,
        error_code,
        stack_frame
    );

    let page = Page::containing_address(VirtAddr::new(faulting_address));

    // check for stack growth
    if stack_pointer - 64 <= faulting_address && faulting_address < *HHDM_OFFSET {
        create_mapping(page, &mut mapper);
    }

    // else, we panic? or smth, report some error

    //panic!("PAGE FAULT!");
}

extern "x86-interrupt" fn timer_handler(_: InterruptStackFrame) {
    x2apic::send_eoi();
}
