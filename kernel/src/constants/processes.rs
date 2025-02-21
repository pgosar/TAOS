pub const INFINITE_LOOP: &[u8] = include_bytes!("../processes/test_binaries/rand_regs");
pub const SYSCALL_BINARY: &[u8] = include_bytes!("../processes/test_binaries/syscall_test");
pub const LONG_LOOP: &[u8] = include_bytes!("../processes/test_binaries/long_loop_print");
pub const SYSCALL_MMAP_MEMORY: &[u8] = include_bytes!("../processes/test_binaries/mmap");
pub const RAND_REGS_EXIT: &[u8] = include_bytes!("../processes/test_binaries/rand_regs_exit");

pub const STACK_START: u64 = 0x7000_0000_0000;
pub const STACK_SIZE: usize = 2 * 4096; // 2 pages for the stack
pub const MAX_FILES: usize = 1024;
