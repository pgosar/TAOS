extern crate alloc;

use crate::{
    interrupts::gdt,
    memory::{frame_allocator::alloc_frame, HHDM_OFFSET, MAPPER},
    processes::{loader::load_elf, registers::Registers},
    serial_println,
};
use alloc::{collections::BTreeMap, sync::Arc};
use core::{
    arch::naked_asm, cell::UnsafeCell, sync::atomic::{AtomicU32, Ordering}
};
use spin::rwlock::RwLock;
use x86_64::{
    instructions::interrupts,
    structures::paging::{OffsetPageTable, PageTable, PhysFrame, Size4KiB},
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
    let (mut process_mapper, process_pml4_frame) = unsafe { create_process_page_table() };

    let (stack_top, entry_point) = load_elf(elf_bytes, &mut process_mapper, &mut MAPPER.lock());

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
pub unsafe fn create_process_page_table() -> (OffsetPageTable<'static>, PhysFrame<Size4KiB>) {
    let new_pml4_frame = alloc_frame().expect("failed to allocate frame for process PML4");
    let new_pml4_phys = new_pml4_frame.start_address();
    let new_pml4_virt = *HHDM_OFFSET + new_pml4_phys.as_u64();
    let new_pml4_ptr: *mut PageTable = new_pml4_virt.as_mut_ptr();

    // Need to zero out new page table
    (*new_pml4_ptr).zero();

    // Copy higher half kernel mappings
    let mapper = MAPPER.lock();
    let kernel_pml4: &PageTable = mapper.level_4_table();
    for i in 256..512 {
        (*new_pml4_ptr)[i] = kernel_pml4[i].clone();
    }

    (
        OffsetPageTable::new(&mut *new_pml4_ptr, *HHDM_OFFSET),
        new_pml4_frame,
    )
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
            "push rax",
            "push rcx",
            "push rdx",
            "call call_process",
            "pop rdx",
            "pop rcx",
            "pop rax",
            in("rdi") registers.as_ref() as *const Registers,
            in("rsi") user_ds,
            in("rdx") user_cs,
            in("rcx") &(*process).kernel_rip,
            in("r8")  &(*process).kernel_rsp
        );
    }
}

#[naked]
#[allow(undefined_naked_function_abi)]
#[no_mangle]
// unsafe extern "C" fn call_process(registers: *const Registers, user_ds: u64, user_cs: u64, 
//                        kernel_rip: *const u64, kernel_rsp: *const u64) {
unsafe fn call_process() {
    naked_asm!(
        //save callee-saved registers
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
        push rbx
        ",
        "lea r10, [rip + 0x5E]",
        // TODO right now, this constant needs to change with every change to the below code

        "mov [rcx], r10",  // RIP in R10, store
        "mov r11, rsp",    // Move RSP to R11
        "mov [r8], r11",   // store RSP (from R11)

        // Needed for cross-privilege iretq
        "push rsi",        //ss
        "mov rax, [rdi + 120]",
        "push rax",       //userrsp
        "mov rax, [rdi + 136]",
        "push rax",       //rflags
        "push rdx",     //cs
        "mov rax, [rdi + 128]",
        "push rax",       //rip

        //     rdi = 0xffffffff00029c20

        // Restore all registers before entering process
        "mov rax, [rdi]",
        "mov rbx, [rdi+8]",
        "mov rcx, [rdi+16]",
        "mov rdx, [rdi+24]",
        "mov rsi, [rdi+32]",

        "mov r8,  [rdi+48]",
        "mov r9,  [rdi+56]",
        "mov r10, [rdi+64]",
        "mov r11, [rdi+72]",
        "mov r12, [rdi+80]",
        "mov r13, [rdi+88]",
        "mov r14, [rdi+96]",
        "mov r15, [rdi+104]",
        "mov rbp, [rdi+112]",

        "mov rdi, [rdi+40]",

        "sti",      //enable interrupts

        "iretq",    // call process

        "cli",      //disable interrupts
                    //restore callee-saved registers
        "
        pop rbx
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
        ",
        "ret"
    );
}