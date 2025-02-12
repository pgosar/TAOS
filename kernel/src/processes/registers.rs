/// Saves all relevant registers onto the stack except for the following:
/// rsp, rip, and rflags as those are saved by the interrupt handler
#[macro_export]
macro_rules! push_registers {
    () => {{
        unsafe {
            core::arch::asm!(
                "push rax",
                "push rbx",
                "push rcx",
                "push rdx",
                "push rsi",
                "push rdi",
                "push r8",
                "push r9",
                "push r10",
                "push r11",
                "push r12",
                "push r13",
                "push r14",
                "push r15",
                "push rbp",
                options()
            );
        }
    }};
}


#[macro_export]
macro_rules! pop_registers {
    ($regs:expr) => {{
        unsafe {
            core::arch::asm!(
                // Expected stack layout when popping:
                //   [rsp + 0]    -> rbp
                //   [rsp + 8]    -> r15
                //   [rsp + 16]   -> r14
                //   [rsp + 24]   -> r13
                //   [rsp + 32]   -> r12
                //   [rsp + 40]   -> r11
                //   [rsp + 48]   -> r10
                //   [rsp + 56]   -> r9
                //   [rsp + 64]   -> r8
                //   [rsp + 72]   -> rdi
                //   [rsp + 80]   -> rsi
                //   [rsp + 88]   -> rdx
                //   [rsp + 96]   -> rcx
                //   [rsp + 104]  -> rbx
                //   [rsp + 112]  -> rax
                "mov rax, [rsp + 112]",
                "mov [{0}], rax",
                "mov rbx, [rsp + 104]",
                "mov [{0} + 8], rbx",
                "mov rcx, [rsp + 96]",
                "mov [{0} + 16], rcx",
                "mov rdx, [rsp + 88]",
                "mov [{0} + 24], rdx",
                "mov rsi, [rsp + 80]",
                "mov [{0} + 32], rsi",
                "mov rdi, [rsp + 72]",
                "mov [{0} + 40], rdi",
                "mov r8, [rsp + 64]",
                "mov [{0} + 48], r8",
                "mov r9, [rsp + 56]",
                "mov [{0} + 56], r9",
                "mov r10, [rsp + 48]",
                "mov [{0} + 64], r10",
                "mov r11, [rsp + 40]",
                "mov [{0} + 72], r11",
                "mov r12, [rsp + 32]",
                "mov [{0} + 80], r12",
                "mov r13, [rsp + 24]",
                "mov [{0} + 88], r13",
                "mov r14, [rsp + 16]",
                "mov [{0} + 96], r14",
                "mov r15, [rsp + 8]",
                "mov [{0} + 104], r15",
                "mov rbp, [rsp]",
                "mov [{0} + 112], rbp",

                // Restore in reverse order from stack
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
                in(reg) &mut $regs,
                options()
            );
        }
    }};
}


#[macro_export]
macro_rules! load_registers {
  ($regs:expr) => {

  }
}

#[derive(Clone, Copy)]
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