use crate::{constants::syscalls::SYSCALL_EXIT, events::{current_running_event_info, EventInfo}, processes::process::{ProcessState, PROCESS_TABLE}, serial_println};

use super::x2apic;

#[no_mangle]
extern "C" fn dispatch_syscall() {
    //panic!("dispatch syscall");
    let syscall_num: u32;
    unsafe {
        core::arch::asm!("mov {0:r}, rbx", out(reg) syscall_num);
    }

    match syscall_num {
        SYSCALL_EXIT => sys_exit(),
        _ => panic!("Unknown syscall: {}", syscall_num),
    }
}

fn sys_exit() {
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

        ((*pcb).kernel_rsp, (*pcb).kernel_rip)
    };

    unsafe {
        // x2apic::send_eoi(); Is this needed for software interrupts?

        // Restore kernel RSP + PC -> RIP from where it was stored in run/resume process
        core::arch::asm!(
            "mov rsp, {0}",
            "push {1}",
            "stc",          // Use carry flag as sentinel to run_process that we're pre-empting
            "ret",
            in(reg) preemption_info.0,
            in(reg) preemption_info.1
        );
    }
}
