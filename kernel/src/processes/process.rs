extern crate alloc;

use crate::memory::frame_allocator::alloc_frame;
use crate::processes::loader::load_elf;
use crate::serial_println;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;
use x86_64::{
    structures::paging::{OffsetPageTable, PageTable, PhysFrame, Size4KiB},
    VirtAddr,
};

// process counter must be thread-safe
static NEXT_PID: AtomicU32 = AtomicU32::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    New,
    Ready,
    Running,
    Blocked,
    Terminated,
}

#[derive(Debug)]
pub struct PCB {
    pub pid: u32,
    state: ProcessState,
    registers: [u64; 32],
    stack_pointer: u64,
    program_counter: u64,
    pub pml4_frame: PhysFrame<Size4KiB>, // this process' page table
}

// global process table must be thread-safe
lazy_static::lazy_static! {
    #[derive(Debug)]
    pub static ref PROCESS_TABLE: Mutex<BTreeMap<u32, Arc<PCB>>> = Mutex::new(BTreeMap::new());
}

pub fn print_process_table() {
    let table = PROCESS_TABLE.lock();
    serial_println!("\nProcess Table Contents:");
    serial_println!("========================");

    if table.is_empty() {
        serial_println!("No processes found");
        return;
    }

    for (pid, pcb) in table.iter() {
        serial_println!(
            "PID {}: State: {:?}, Registers: {:?}, SP: {:#x}, PC: {:#x}",
            pid,
            pcb.state,
            pcb.registers,
            pcb.stack_pointer,
            pcb.program_counter
        );
    }
    serial_println!("========================");
}

pub fn create_process(
    elf_bytes: &[u8],
    kernel_mapper: &mut OffsetPageTable<'static>,
    hhdm_offset: VirtAddr,
) -> Arc<PCB> {
    let pid = NEXT_PID.fetch_add(1, Ordering::SeqCst);

    let (mut process_mapper, process_pml4_frame) =
        unsafe { create_process_page_table(kernel_mapper, hhdm_offset) };

    let (stack_top, entry_point) = load_elf(elf_bytes, &mut process_mapper);
    let process = Arc::new(PCB {
        pid,
        state: ProcessState::New,
        registers: [0; 32],
        stack_pointer: stack_top.as_u64(),
        program_counter: entry_point,
        pml4_frame: process_pml4_frame,
    });

    PROCESS_TABLE.lock().insert(pid, Arc::clone(&process));
    unsafe { run_process(&process) };
    process
}

pub unsafe fn create_process_page_table(
    kernel_pt: &OffsetPageTable<'static>,
    hhdm_base: VirtAddr,
) -> (OffsetPageTable<'static>, PhysFrame<Size4KiB>) {
    let new_pml4_frame = alloc_frame().expect("failed to allocate frame for process PML4");
    let new_pml4_phys = new_pml4_frame.start_address();
    let new_pml4_virt = hhdm_base + new_pml4_phys.as_u64();
    let new_pml4_ptr: *mut PageTable = new_pml4_virt.as_mut_ptr();

    // Need to zero out new page table
    (*new_pml4_ptr).zero();

    // Copy higher half kernel mappings
    let kernel_pml4: &PageTable = kernel_pt.level_4_table();
    // FIXME: really this should only be 256..512 but why does that page fault and this works
    for i in 256..512 {
        (*new_pml4_ptr)[i] = kernel_pml4[i].clone();
    }

    Cr3::write(new_pml4_frame, Cr3Flags::empty());

    (
        OffsetPageTable::new(&mut *new_pml4_ptr, hhdm_base),
        new_pml4_frame,
    )
}

// Writes new page table entry to cr3 to load process
use core::arch::asm;
use x86_64::registers::control::{Cr3, Cr3Flags};

unsafe fn run_process(process: &PCB) -> ! {
    serial_println!("RUNNING PROCESS!");
    asm!("mov rsp, {}", in(reg) process.stack_pointer);
    asm!("jmp {}", in(reg) process.program_counter);

    // Should never reach
    loop {
        asm!("hlt");
    }
}
