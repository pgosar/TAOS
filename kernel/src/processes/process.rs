extern crate alloc;

use crate::{
    interrupts::gdt,
    memory::{
        frame_allocator::{alloc_frame, with_bitmap_frame_allocator, with_generic_allocator},
        HHDM_OFFSET, MAPPER,
    },
    processes::{loader::load_elf, registers::Registers},
    restore_registers_into_stack, serial_println,
};
use alloc::{collections::BTreeMap, sync::Arc};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicU32, Ordering},
};
use spin::rwlock::RwLock;
use x86_64::{
    instructions::interrupts,
    structures::paging::{FrameDeallocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB},
};

// process counter must be thread-safe
// PID 0 will ONLY be used for errors/PID not found
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
    pub state: ProcessState,
    pub kernel_rsp: u64,
    pub kernel_rip: u64,
    pub registers: Arc<Registers>,
    pub pml4_frame: PhysFrame<Size4KiB>, // this process' page table
}

pub struct UnsafePCB {
    pub pcb: UnsafeCell<PCB>,
}
impl UnsafePCB {
    fn init(pcb: PCB) -> Self {
        UnsafePCB {
            pcb: UnsafeCell::new(pcb),
        }
    }
}

unsafe impl Sync for UnsafePCB {}
type ProcessTable = Arc<RwLock<BTreeMap<u32, Arc<UnsafePCB>>>>;

// global process table must be thread-safe
lazy_static::lazy_static! {
    #[derive(Debug)]
    pub static ref PROCESS_TABLE: ProcessTable = Arc::new(RwLock::new(BTreeMap::new()));
}

impl PCB {
    /// Creates a page table mapper for temporary use during only process creation and cleanup
    /// # Safety
    /// TODO
    pub unsafe fn create_mapper(&mut self) -> OffsetPageTable<'_> {
        let virt = *HHDM_OFFSET + self.pml4_frame.start_address().as_u64();
        let ptr = virt.as_mut_ptr::<PageTable>();
        OffsetPageTable::new(unsafe { &mut *ptr }, *HHDM_OFFSET)
    }
}

/// # Safety
///
/// TODO
pub unsafe fn print_process_table(process_table: &PROCESS_TABLE) {
    let table = process_table.read();
    serial_println!("\nProcess Table Contents:");
    serial_println!("========================");

    if table.is_empty() {
        serial_println!("No processes found");
        return;
    }

    for (pid, pcb) in table.iter() {
        let pcb = pcb.pcb.get();
        serial_println!(
            "PID {}: State: {:?}, Registers: {:?}, SP: {:#x}, PC: {:#x}",
            pid,
            (*pcb).state,
            (*pcb).registers,
            (*pcb).registers.rsp,
            (*pcb).registers.rip
        );
    }
    serial_println!("========================");
}

pub fn create_process(elf_bytes: &[u8]) -> u32 {
    let pid = NEXT_PID.fetch_add(1, Ordering::SeqCst);

    // Build a new process address space
    let process_pml4_frame = unsafe { create_process_page_table() };
    let mut mapper = unsafe {
        let virt = *HHDM_OFFSET + process_pml4_frame.start_address().as_u64();
        let ptr = virt.as_mut_ptr::<PageTable>();
        OffsetPageTable::new(&mut *ptr, *HHDM_OFFSET)
    };
    let (stack_top, entry_point) = load_elf(elf_bytes, &mut mapper, &mut MAPPER.lock());

    let process = Arc::new(UnsafePCB::init(PCB {
        pid,
        state: ProcessState::New,
        kernel_rsp: 0,
        kernel_rip: 0,
        registers: Arc::new(Registers {
            rax: 0,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rbp: 0,
            rsp: stack_top.as_u64(),
            rip: entry_point,
            rflags: 0x202,
        }),
        pml4_frame: process_pml4_frame,
    }));
    let pid = unsafe { (*process.pcb.get()).pid };
    PROCESS_TABLE.write().insert(pid, Arc::clone(&process));
    serial_println!("Created process with PID: {}", pid);
    // schedule process (call from main)
    pid
}

/// # Safety
///
/// TODO
unsafe fn create_process_page_table() -> PhysFrame<Size4KiB> {
    let frame = alloc_frame().expect("Failed to allocate PML4 frame");
    let virt = *HHDM_OFFSET + frame.start_address().as_u64();
    let ptr = virt.as_mut_ptr::<PageTable>();

    // Initialize and copy kernel mappings
    let mapper = MAPPER.lock();
    unsafe {
        (*ptr).zero();
        let kernel_pml4 = mapper.level_4_table();
        for i in 256..512 {
            (*ptr)[i] = kernel_pml4[i].clone();
        }
    }

    frame
}

use core::arch::asm;
use x86_64::registers::control::{Cr3, Cr3Flags};

/// run a process in ring 3
/// # Safety
///
/// TODO
#[no_mangle]
pub async unsafe fn run_process_ring3(pid: u32) {
    interrupts::disable();

    serial_println!("RUNNING PROCESS");
    let process = {
        let process_table = PROCESS_TABLE.read();
        let process = process_table
            .get(&pid)
            .expect("Could not find process from process table");
        process.clone()
    };

    // Do not lock lowest common denominator
    // Once kernel threads are in, will need lock around PCB
    // But not TCB
    let process = process.pcb.get();

    (*process).state = ProcessState::Running;

    Cr3::write((*process).pml4_frame, Cr3Flags::empty());

    let user_cs = gdt::GDT.1.user_code_selector.0 as u64;
    let user_ds = gdt::GDT.1.user_data_selector.0 as u64;

    let registers = &(*process).registers.clone();

    // Stack layout to move into user mode
    unsafe {
        restore_registers_into_stack!(registers);

        asm!(
            "lea r10, [rip + 0x34]",
            // TODO right now, this constant needs to change with every change to the below code

            "mov [rsi], r10",  // RIP in R10, store

            "mov r11, rsp",         // Move RSP to R10
            "mov [r9], r11", // store RSP (from R11)

            // Needed for cross-privilege iretq
            "push r8",        //ss
            "push rcx",       //userrsp
            "push rax",       //rflags
            "push rdi",       //cs
            "push rdx",       //rip

            // Restore all registers before entering process
            "sub rsp, 144",
            "pop rax",
            "pop rbx",
            "pop rcx",
            "pop rdx",
            "pop rsi",
            "pop rdi",
            "pop r8",
            "pop r9",
            "pop r10",
            "pop r11",
            "pop r12",
            "pop r13",
            "pop r14",
            "pop r15",
            "pop rbp",
            "add rsp, 24",

            "sti",      //re-enable interrupts

            "iretq",    // call process
            "cli",

            in("r8") user_ds,
            in("rcx") registers.rsp,
            in("rax") registers.rflags,
            in("rdi") user_cs,
            in("rdx") registers.rip,

            in("rsi") &(*process).kernel_rip,
            in("r9") &(*process).kernel_rsp,
            options(nostack)
        );
    }
}

/// Clear the PML4 associated with the PCB
///
/// * `pcb`: The process PCB to clear memory for
pub fn clear_process_frames(pcb: &mut PCB) {
    let pml4_frame = pcb.pml4_frame;
    let mapper = unsafe { pcb.create_mapper() };

    with_generic_allocator(|deallocator| {
        // Iterate over first 256 entries (user space)
        for i in 0..256 {
            let entry = &mapper.level_4_table()[i];
            if entry.is_unused() {
                continue;
            }

            let pdpt_frame = PhysFrame::containing_address(entry.addr());
            unsafe {
                free_page_table(pdpt_frame, 3, deallocator, HHDM_OFFSET.as_u64());
            }
        }
        unsafe { deallocator.deallocate_frame(pml4_frame) };
    });

    with_bitmap_frame_allocator(|alloc| {
        alloc.print_bitmap_free_frames();
    });
}

/// Helper function to recursively multi level page tables
///
/// * `frame`: the current page table frame iterating over
/// * `level`: the current level of the page table we're on
/// * `deallocator`:
/// * `hhdm_offset`:
unsafe fn free_page_table(
    frame: PhysFrame,
    level: u8,
    deallocator: &mut impl FrameDeallocator<Size4KiB>,
    hhdm_offset: u64,
) {
    let virt = hhdm_offset + frame.start_address().as_u64();
    let table = unsafe { &mut *(virt as *mut PageTable) };

    for entry in table.iter_mut() {
        if entry.is_unused() {
            continue;
        }

        if level > 1 {
            let child_frame = PhysFrame::containing_address(entry.addr());
            free_page_table(child_frame, level - 1, deallocator, hhdm_offset);
        } else {
            // Free level one page
            let page_frame = PhysFrame::containing_address(entry.addr());
            deallocator.deallocate_frame(page_frame);
        }
        entry.set_unused();
    }
    deallocator.deallocate_frame(frame);
}
