use core::ffi::CStr;

use x86_64::registers::{model_specific::{GsBase, KernelGsBase}, segmentation::{Segment64, GS}};

use crate::{
    constants::{gdt::RING0_STACK_SIZE, syscalls::*}, debug, events::{current_running_event_info, EventInfo}, interrupts::gdt::TSSS, processes::process::{clear_process_frames, ProcessState, PROCESS_TABLE}, serial_println
};

#[warn(unused)]
use crate::interrupts::x2apic;
#[allow(unused)]
use core::arch::naked_asm;

#[repr(C)]
#[derive(Debug)]
pub struct SyscallRegisters {
    pub number: u64, // syscall number (originally in rax)
    pub arg1: u64,   // originally in rdi
    pub arg2: u64,   // originally in rsi
    pub arg3: u64,   // originally in rdx
    pub arg4: u64,   // originally in r10
    pub arg5: u64,   // originally in r8
    pub arg6: u64,   // originally in r9
}

// #[no_mangle]
// pub fn syscall_handler_64(syscall: *const SyscallRegisters) {
//     let syscall = unsafe { &*syscall };
//     serial_println!("Running syscall handler {}", syscall.number);
//     serial_println!("GS: {}", KernelGsBase::read().as_u64());
//     unsafe { 
//         core::arch::asm!(
//             "mov rsp, {0}",
//             "sub rsp, 56",
//             "mov [rsp + 0], {1}",
//             "mov [rsp + 8], {2}",
//             "mov [rsp + 16], {3}",
//             "mov [rsp + 24], {4}",
//             "mov [rsp + 32], {5}",
//             "mov [rsp + 40], {6}",
//             "mov [rsp + 48], {7}",
//             "call syscall_handler_impl",
//             "add rsp, 56",
//             in(reg) (TSSS[0].privilege_stack_table[0] + RING0_STACK_SIZE as u64).as_u64(),
//             in (reg) syscall.number,
//             in (reg) syscall.arg1,
//             in (reg) syscall.arg2,
//             in (reg) syscall.arg3,
//             in (reg) syscall.arg4,
//             in (reg) syscall.arg5,
//             in (reg) syscall.arg6,
//         ) 
//     }
//             // "call syscall_handler_impl",
//             // in(reg)             // kernel_stack = in (reg) (TSSS[0].privilege_stack_table[0] + RING0_STACK_SIZE as u64).as_u64()
// }

#[naked]
#[no_mangle]
pub extern "C" fn syscall_handler_64_naked() {
    unsafe {
        core::arch::naked_asm!(
            "swapgs",
            "cli", // disables interrupts, unsure if needed
            "mov r12, rcx",
            "mov r13, r11",
            "mov rsp, 0xffffffff800c02f8",
            "push rbx",
            "sub rsp, 56",
            // gets num and args
            "mov [rsp + 0], rax",
            "mov [rsp + 8], rdi",
            "mov [rsp + 16], rsi",
            "mov [rsp + 24], rdx",
            "mov [rsp + 32], r10",
            "mov [rsp + 40], r8",
            "mov [rsp + 48], r9",
            "mov rdi, rsp",
            "call syscall_handler_impl",
            "add rsp, 56",
            "pop rbx",
            "mov rcx, r12",
            "mov r11, r13",
            "swapgs",
            "sysretq",
        )
    };
}

#[no_mangle]
pub fn syscall_handler_impl(syscall: *const SyscallRegisters) {
    let syscall = unsafe { &*syscall };
    serial_println!("Syscall num: {}", syscall.number);
    match syscall.number as u32 {
        SYSCALL_EXIT => sys_exit(syscall.arg1),
        SYSCALL_PRINT => sys_print(syscall.arg1 as *const u8),
        _ => {
            panic!("Unknown syscall, {}", syscall.number);
        }
    }
}

pub fn sys_exit(code: u64) {
    // TODO handle hierarchy (parent processes), resources, threads, etc.
    let cpuid: u32 = x2apic::current_core_id() as u32;
    let event: EventInfo = current_running_event_info(cpuid);

    serial_println!("Process {} exitted with code {}", event.pid, code);

    if event.pid == 0 {
        panic!("Calling exit from outside of process");
    }

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

// Not a real system call, but useful for testing
pub fn sys_print(buffer: *const u8) {
    // let c_str = unsafe { CStr::from_ptr(buffer as *const i8) };
    // let str_slice = c_str.to_str().expect("Invalid UTF-8 string");
    // serial_println!("Buffer: {}", str_slice);
    //
    serial_println!("HI");
}
