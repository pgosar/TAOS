use crate::{
    constants::syscalls::{SYSCALL_EXIT, SYSCALL_NANOSLEEP, SYSCALL_PRINT}, debug, events::{current_running_event_info, nanosleep_current_event, EventInfo}, processes::process::{clear_process_frames, ProcessState, PROCESS_TABLE}, serial_println
};


#[no_mangle]
extern "C" fn dispatch_syscall() {
    let syscall_num: u32;
    let arg1: u64;
    unsafe {
        core::arch::asm!(
            "mov {0:r}, rax",
            "mov {1}, rbx",
            out(reg) syscall_num,
            out(reg) arg1
        );
    }

    match syscall_num {
        SYSCALL_EXIT => sys_exit(),
        SYSCALL_NANOSLEEP => sys_nanosleep(arg1),
        SYSCALL_PRINT => serial_println!("Hello world!"),
        _ => panic!("Unknown syscall: {}", syscall_num),
    }
}

fn sys_exit() {
    // TODO handle hierarchy (parent processes), resources, threads, etc.
    // TODO recursive page table walk to handle cleaning up process memory
    let event: EventInfo = current_running_event_info();

    if event.pid == 0 {
        panic!("Calling exit from outside of process");
    }

    debug!("Process {} exit", event.pid);

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
}

fn sys_nanosleep(nanos: u64) {
    nanosleep_current_event(nanos);
}
