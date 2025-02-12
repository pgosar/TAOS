pub const BINARY: &[u8] = include_bytes!("../processes/test_binaries/rax");

pub const STACK_START: u64 = 0x7000_0000_0000;
pub const STACK_SIZE: usize = 2 * 4096; // 4 pages for the stack
