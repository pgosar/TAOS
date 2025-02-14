extern crate alloc;

use crate::interrupts::gdt;
use crate::memory::frame_allocator::alloc_frame;
use crate::processes::{loader::load_elf, registers::Registers};
use crate::serial_println;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::rwlock::RwLock;
use x86_64::instructions::interrupts;
use x86_64::{
    structures::paging::{OffsetPageTable, PageTable, PhysFrame, Size4KiB},
    VirtAddr,
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

pub fn create_process(
    elf_bytes: &[u8],
    kernel_mapper: &mut OffsetPageTable<'static>,
    hhdm_offset: VirtAddr,
) -> u32 {
    let pid = NEXT_PID.fetch_add(1, Ordering::SeqCst);

    // Build a new process address space
    let (mut process_mapper, process_pml4_frame) =
        unsafe { create_process_page_table(kernel_mapper, hhdm_offset) };

    let (stack_top, entry_point) = load_elf(elf_bytes, &mut process_mapper, kernel_mapper);

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
    let pid = unsafe {(*process.pcb.get()).pid };
    PROCESS_TABLE.write().insert(pid, Arc::clone(&process));
    serial_println!("Created process with PID: {}", pid);
    // schedule process (call from main)
    pid
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
    for i in 256..512 {
        (*new_pml4_ptr)[i] = kernel_pml4[i].clone();
    }

    (
        OffsetPageTable::new(&mut *new_pml4_ptr, hhdm_base),
        new_pml4_frame,
    )
}

use core::arch::asm;
use x86_64::registers::control::{Cr3, Cr3Flags};

// run a process in ring 3
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

    asm!(
        "mov rax, {rax}",
        "mov rbx, {rbx}",
        "mov rcx, {rcx}",
        "mov rdx, {rdx}",
        "mov rsi, {rsi}",
        "mov rdi, {rdi}",
        "mov r8, {r8}",
        "mov r9, {r9}",
        "mov r10, {r10}",
        "mov r11, {r11}",
        "mov r12, {r12}",
        "mov r13, {r13}",
        "mov r14, {r14}",
        "mov r15, {r15}",

        rax = in(reg) registers.rax,
        rbx = in(reg) registers.rbx,
        rcx = in(reg) registers.rcx,
        rdx = in(reg) registers.rdx,
        rsi = in(reg) registers.rsi,
        rdi = in(reg) registers.rdi,
        r8 = in(reg) registers.r8,
        r9 = in(reg) registers.r9,
        r10 = in(reg) registers.r10,
        r11 = in(reg) registers.r11,
        r12 = in(reg) registers.r12,
        r13 = in(reg) registers.r13,
        r14 = in(reg) registers.r14,
        r15 = in(reg) registers.r15,
    );

    // Stack layout to move into user mode
    unsafe {
        asm!(
            "clc",
            "jmp 2f",
            "4:",

            "mov [{pcb_pc}], rax",     
            "mov rax, rsp",            
            // "add rax, 8",              
            "mov [{pcb_rsp}], rax", 

            // Needed for cross-privilege iretq
            "push {ss}",
            "push {userrsp}",
            "push {rflags}",
            "push {cs}",
            "push {rip}",

            "sti",                  

            "iretq",                 
            // will store program counter to return back to scheduling code
            "2:",
            "call 3f",
            "3:",
            "nop",   // Also for debug purposes (single-byte NOP opcode is 0x90)
            "jb 5f",
            "pop rax", 
            "jae 4b",
            "5:",
            "cli",

            ss = in(reg) user_ds,
            userrsp = in(reg) registers.rsp,
            rflags = in(reg) registers.rflags,
            cs = in(reg) user_cs,
            rip = in(reg) registers.rip,

            pcb_pc = in(reg) &(*process).kernel_rip,
            pcb_rsp = in(reg) &(*process).kernel_rsp,
            out("rax") _
        );
    }

    // rust compiler generates this by default
    // The address of this will be program counter + 4 from the iretq instruction in the load registers macro
    // return Poll::Ready(())
    serial_println!("Returned from process")
}
