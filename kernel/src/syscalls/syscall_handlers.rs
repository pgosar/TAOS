use crate::{
    constants::syscalls::{SYSCALL_EXIT, SYSCALL_MMAP, SYSCALL_PRINT},
    events::{current_running_event_info, EventInfo},
    processes::process::{clear_process_frames, ProcessState, PROCESS_TABLE},
    serial_println,
    syscalls::mmap::*,
};

use crate::interrupts::x2apic;

#[no_mangle]
extern "C" fn dispatch_syscall() {
    let syscall_num: u32;
    let param_1: u64 = 0;
    let param_2: u64 = 0;
    let param_3: u64 = 0;
    let param_4: u64 = 0;
    let param_5: u64 = 0;
    let param_6: u64 = 0;
    unsafe {
        core::arch::asm!(
            "mov {0}, rax",
            "mov rdi, {1}",
            "mov rsi, {2}",
            "mov rdx, {3}",
            "mov r10, {4}",
            "mov r8, {5}",
            "mov r9, {6}",
            out(reg) syscall_num,
            in(reg) param_1,
            in(reg) param_2,
            in(reg) param_3,
            in(reg) param_4,
            in(reg) param_5,
            in(reg) param_6,
        );
    }

    match syscall_num {
        SYSCALL_EXIT => sys_exit(),
        SYSCALL_MMAP => sys_mmap(param_1, param_2, param_3, param_4, param_5 as i64, param_6),
        _ => panic!("Unknown syscall: {}", syscall_num),
    };
}

fn sys_exit<T>() -> Option<T> {
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
