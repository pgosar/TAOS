use crate::{
    constants::syscalls::{SYSCALL_EXIT, SYSCALL_MMAP},
    events::{current_running_event_info, EventInfo},
    interrupts::x2apic::send_eoi,
    processes::process::{clear_process_frames, ProcessState, PROCESS_TABLE},
    serial_println,
    syscalls::mmap::*,
};

use crate::interrupts::x2apic;

#[no_mangle]
extern "C" fn dispatch_syscall() {
    let syscall_num: u32;
    let param_1: u64;
    let param_2: u64;
    let param_3: u64;
    let param_4: u64;
    let param_5: u64;
    let param_6: u64;
    unsafe {
        core::arch::asm!(
            "mov {0:r}, rax",
            "mov {1}, rdi",
            "mov {2}, rsi",
            "mov {3}, rdx",
            "mov {4}, r10",
            "mov {5}, r9",
            "mov {6}, r8",
            out(reg) syscall_num,
            out(reg) param_1,
            out(reg) param_2,
            out(reg) param_3,
            out(reg) param_4,
            out(reg) param_5,
            out(reg) param_6,
        );
    }

    match syscall_num {
        SYSCALL_EXIT => sys_exit(param_1, param_2, param_3, param_4, param_5, param_6),
        SYSCALL_MMAP => sys_mmap(param_1, param_2, param_3, param_4, param_5 as i64, param_6),
        _ => panic!("Unknown syscall: {}", syscall_num),
    };
    send_eoi();
}

fn sys_exit<T>(p1: u64, p2: u64, p3: u64, p4: u64, p5: u64, p6: u64) -> Option<T> {
    // TODO handle hierarchy (parent processes), resources, threads, etc.
    // TODO recursive page table walk to handle cleaning up process memory
    let cpuid: u32 = x2apic::current_core_id() as u32;
    let event: EventInfo = current_running_event_info(cpuid);

    serial_println!("CODE: {}", p1);
    serial_println!("CODE: {}", p2);
    serial_println!("CODE: {}", p3);
    serial_println!("CODE: {}", p4);
    serial_println!("CODE: {}", p5);
    serial_println!("CODE: {}", p6);

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

        (*pcb).state = ProcessState::Terminated;
        clear_process_frames(&mut *pcb);
        process_table.remove(&event.pid);
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
    None
}
