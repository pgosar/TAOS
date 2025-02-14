/// Saves all relevant registers onto the stack except for the following:
/// rsp, rip, and rflags as those are saved by the interrupt handler
#[macro_export]
macro_rules! push_registers {
    () => {{
        unsafe {
            core::arch::asm!(
                "sub rsp, 472",
                "mov [rsp], rbp",
                "mov [rsp + 8], r15",
                "mov [rsp + 16], r14",
                "mov [rsp + 24], r13",
                "mov [rsp + 32], rsi",
                "mov [rsp + 40], r12",
                "mov [rsp + 48], r11",
                "mov [rsp + 56], r10",
                "mov [rsp + 64], r9",
                "mov [rsp + 72], r8",
                "mov [rsp + 80], rsi",
                "mov [rsp + 88], rdx",
                "mov [rsp + 96], rcx",
                "mov [rsp + 104], rbp",
                "mov [rsp + 112], rax",
                "add rsp, 472",
                // "push rax",
                // "push rbx"
                // "push rcx",
                // "push rdx",
                // "push rsi",
                // "push rdi",
                // "push r8",
                // "push r9",
                // "push r10",
                // "push r11",
                // "push r12",
                // "push r13",
                // "push r14",
                // "push r15",
                // "push rbp",
                options(preserves_flags),
            );
        }
    }};
}

#[macro_export]
macro_rules! pop_registers {
    () => {{
        unsafe {
            core::arch::asm!(
                "pop rbp",
                "pop r15",
                "pop r14",
                "pop r13",
                "pop r12",
                "pop r11",
                "pop r10",
                "pop r9",
                "pop r8",
                "pop rdi",
                "pop rsi",
                "pop rdx",
                "pop rcx",
                "pop rbx",
                "pop rax",
                options(nostack)
            );
        }
    }};
}

#[macro_export]
macro_rules! load_registers {
    ($regs:expr) => {};
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Registers {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub rip: u64,
    pub rflags: u64,
}

impl Registers {
    pub fn new() -> Self {
        Self {
            rax: 0,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rbp: 0,
            rsp: 0,
            rip: 0,
            rflags: 0,
        }
    }
}

impl Default for Registers {
    fn default() -> Self {
        Self::new()
    }
}

impl alloc::fmt::Debug for Registers {
    fn fmt(&self, f: &mut alloc::fmt::Formatter) -> alloc::fmt::Result {
        let mut ds = f.debug_struct("Registers");

        ds.field("rax", &format_args!("{:#016x}", self.rax))
            .field("rbx", &format_args!("{:#016x}", self.rbx))
            .field("rcx", &format_args!("{:#016x}", self.rcx))
            .field("rdx", &format_args!("{:#016x}", self.rdx))
            .field("rsi", &format_args!("{:#016x}", self.rsi))
            .field("rdi", &format_args!("{:#016x}", self.rdi))
            .field("r8", &format_args!("{:#016x}", self.r8))
            .field("r9", &format_args!("{:#016x}", self.r9))
            .field("r10", &format_args!("{:#016x}", self.r10))
            .field("r11", &format_args!("{:#016x}", self.r11))
            .field("r12", &format_args!("{:#016x}", self.r12))
            .field("r13", &format_args!("{:#016x}", self.r13))
            .field("r14", &format_args!("{:#016x}", self.r14))
            .field("r15", &format_args!("{:#016x}", self.r15))
            .field("rbp", &format_args!("{:#016x}", self.rbp))
            .field("rsp", &format_args!("{:#016x}", self.rsp))
            .field("rip", &format_args!("{:#016x}", self.rip))
            .field("rflags", &format_args!("{:#016x}", self.rflags));

        ds.finish()
    }
}
