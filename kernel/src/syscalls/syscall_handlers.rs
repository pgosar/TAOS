use crate::{
    constants::syscalls::SYSCALL_EXIT,
    events::{current_running_event_info, EventInfo},
    memory::{
        frame_allocator::{GlobalFrameAllocator, FRAME_ALLOCATOR},
        paging::clear_process_frames,
    },
    processes::process::{ProcessState, PROCESS_TABLE},
    serial_println,
};

use crate::interrupts::x2apic;

#[no_mangle]
extern "C" fn dispatch_syscall() {
    let syscall_num: u32;
    unsafe {
        core::arch::asm!("mov {0:r}, rax", out(reg) syscall_num);
    }

    match syscall_num {
        SYSCALL_EXIT => sys_exit(),
        _ => panic!("Unknown syscall: {}", syscall_num),
    }
}

fn sys_exit() {
    // TODO handle hierarchy (parent processes), resources, threads, etc.
    // TODO recursive page table walk to handle cleaning up process memory
    let cpuid: u32 = x2apic::current_core_id() as u32;
    let event: EventInfo = current_running_event_info(cpuid);

    if event.pid == 0 {
        panic!("Calling exit from outside of process");
    }

    serial_println!("Process {} exit", event.pid);

    // Get PCB from PID
    let preemption_info = unsafe {
        let mut process_table = PROCESS_TABLE.write();
        let process = process_table
            .get_mut(&event.pid)
            .expect("Process not found");

        let pcb = process.pcb.get();

        let mut allocator_lock = FRAME_ALLOCATOR.lock();
        let bitmap_allocator = match &mut *allocator_lock {
            Some(GlobalFrameAllocator::Bitmap(bitmap_alloc)) => bitmap_alloc,
            _ => panic!("Allocator is not a BitmapFrameAllocator"),
        };
        bitmap_allocator.print_bitmap();
        drop(allocator_lock);
        (*pcb).state = ProcessState::Terminated;
        clear_process_frames(&mut *pcb);
        ((*pcb).kernel_rsp, (*pcb).kernel_rip)
    };

    unsafe {
        // Restore kernel RSP + PC -> RIP from where it was stored in run/resume process
        core::arch::asm!(
            "mov rsp, {0}",
            "push {1}",
            "stc",          // Use carry flag as sentinel to run_process that we're exiting
            "ret",
            in(reg) preemption_info.0,
            in(reg) preemption_info.1
        );
    }
}
