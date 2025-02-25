use core::sync::atomic::Ordering;

use x86_64::structures::paging::PageTable;

use crate::{
    events::{current_running_event_info},
    interrupts::x2apic,
    memory::{frame_allocator::alloc_frame, HHDM_OFFSET},
    processes::process::{NEXT_PID, PROCESS_TABLE}, serial_println,
};

pub fn sys_fork() -> u64 {
    let child_pid = NEXT_PID.fetch_add(1, Ordering::SeqCst);
    let cpuid: u32 = x2apic::current_core_id() as u32;
    let parent_pid = current_running_event_info(cpuid).pid;
    let process_table = PROCESS_TABLE.read();
    let process = process_table
        .get(&parent_pid)
        .expect("can't find pcb in process table");
    let parent_pcb = process.pcb.get();

    let child_pcb = parent_pcb.clone();
    serial_println!("CHILD PCB {:?}", child_pcb);

    let parent_frame = unsafe { (*parent_pcb).pml4_frame };

    let frame = alloc_frame().expect("Failed to allocate PML4 frame");
    let virt = *HHDM_OFFSET + frame.start_address().as_u64();
    let ptr = virt.as_mut_ptr::<PageTable>();

    return child_pid as u64;
}
