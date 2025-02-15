use crate::constants::syscalls::SYSCALL_EXIT;

#[no_mangle]
extern "C" fn dispatch_syscall() {
    //panic!("dispatch syscall");
    let syscall_num: u32;
    unsafe {
        core::arch::asm!("mov {}, rbx", out(reg) syscall_num);
    }

    match syscall_num {
        SYSCALL_EXIT => sys_exit(),
        _ => panic!("Unknown syscall: {}", syscall_num),
    }
}

fn sys_exit() {
    panic!("SYS_EXIT NOT IMPLEMENTED")
}
