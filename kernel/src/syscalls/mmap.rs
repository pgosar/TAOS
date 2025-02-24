use alloc::vec::Vec;
use x86_64::{
    structures::paging::{Page, PageTableFlags, Size4KiB},
    VirtAddr,
};

use crate::{
    constants::{memory::PAGE_SIZE, syscalls::START_MMAP_ADDRESS},
    events::{current_running_event_info, EventInfo},
    interrupts::x2apic,
    memory::{paging::create_not_present_mapping, HHDM_OFFSET, MAPPER},
    processes::process::PROCESS_TABLE,
    serial_println,
};

static mut MMAP_ADDR: u64 = START_MMAP_ADDRESS;

#[derive(Clone, Debug)]
pub struct MmapCall {
    pub start: u64,
    pub end: u64,
    pub fd: i64,
    pub offset: u64,
    pub loaded: Vec<bool>,
}

impl MmapCall {
    pub fn new(start: u64, end: u64, fd: i64, offset: u64) -> Self {
        MmapCall {
            start,
            end,
            fd,
            offset,
            loaded: Vec::new(),
        }
    }

    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.start && addr < self.end
    }
}

// See https://www.man7.org/linux/man-pages/man2/mmap.2.html
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmapFlags(u64);

impl MmapFlags {
    pub const MAP_SHARED: u64 = 1 << 0;
    pub const MAP_SHARED_VALIDATE: u64 = 1 << 1;
    pub const MAP_PRIVATE: u64 = 1 << 2;
    pub const MAP_32BIT: u64 = 1 << 3;
    pub const MAP_ANONYMOUS: u64 = 1 << 4;
    pub const MAP_ANON: u64 = 1 << 4;
    pub const MAP_DENYWRITE: u64 = 1 << 5;
    pub const MAP_EXECUTABLE: u64 = 1 << 6;
    pub const MAP_FILE: u64 = 1 << 7;
    pub const MAP_FIXED: u64 = 1 << 8;
    pub const MAP_FIXED_NOREPLACE: u64 = 1 << 9;
    pub const MAP_GROWSDOWN: u64 = 1 << 10;
    pub const MAP_HUGETLB: u64 = 1 << 11;
    pub const MAP_HUGE_2MB: u64 = 1 << 12;
    pub const MAP_HUGE_1GB: u64 = 1 << 13;
    pub const MAP_LOCKED: u64 = 1 << 14;
    pub const MAP_NONBLOCK: u64 = 1 << 15;
    pub const MAP_NORESERVE: u64 = 1 << 16;
    pub const MAP_POPULATE: u64 = 1 << 17;
    pub const MAP_STACK: u64 = 1 << 18;
    pub const MAP_SYNC: u64 = 1 << 19;
    pub const MAP_UNINITIALIZED: u64 = 1 << 20;

    pub const fn new() -> Self {
        MmapFlags(0)
    }

    // creates MmapFlags with inputted flags
    pub const fn with_flags(self, flag: u64) -> Self {
        MmapFlags(self.0 | flag)
    }

    // Checks if MmapFlags contains input flags
    pub const fn contains(self, flag: u64) -> bool {
        (self.0 & flag) != 0
    }

    // returns the MmapFlags
    pub const fn bits(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtFlags(u64);

impl ProtFlags {
    pub const PROT_EXEC: u64 = 1 << 0;
    pub const PROT_READ: u64 = 1 << 1;
    pub const PROT_WRITE: u64 = 1 << 2;
    pub const PROT_NONE: u64 = 1 << 3;

    pub const fn new() -> Self {
        ProtFlags(0)
    }

    // creates ProtFlags with inputted flags
    pub const fn with_flags(self, flag: u64) -> Self {
        ProtFlags(self.0 | flag)
    }

    // Checks if ProtFlags contains input flags
    pub const fn contains(self, flag: u64) -> bool {
        (self.0 & flag) != 0
    }

    // returns the ProtFlags
    pub const fn bits(self) -> u64 {
        self.0
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MmapErrors {
    EACCES,
    EAGAIN,
    EBADF,
    EEXIST,
    EINVAL,
    ENFILE,
    ENODEV,
    ENOMEM,
    EOVERFLOW,
    EPERM,
    ETXTBSY,
    SIGSEGV,
    SIGBUS,
}

pub fn protection_to_pagetable_flags(prot: u64) -> PageTableFlags {
    let mut flags = PageTableFlags::empty();

    if prot & ProtFlags::PROT_READ != 0 {
        flags |= PageTableFlags::PRESENT;
    }
    if prot & ProtFlags::PROT_WRITE != 0 {
        flags |= PageTableFlags::WRITABLE | PageTableFlags::PRESENT;
    }
    if prot & ProtFlags::PROT_EXEC == 0 {
        flags |= PageTableFlags::NO_EXECUTE | PageTableFlags::PRESENT;
    }

    flags
}

pub fn sys_mmap(
    addr: u64,
    len: u64,
    prot: u64,
    flags: u64,
    fd: i64,
    offset: u64,
) -> Option<VirtAddr> {
    serial_println!("Address: {}", addr);
    serial_println!("Len: {}", len);
    serial_println!("Prot: {}", prot);
    serial_println!("Flags: {}", flags);
    serial_println!("FD: {}", fd);
    serial_println!("Offset: {}", offset);
    // we will currently completely ignore user address requests and do whatever we want
    if len == 0 {
        serial_println!("Zero length mapping");
        panic!();
        // return something
    }
    let cpuid: u32 = x2apic::current_core_id() as u32;
    let event: EventInfo = current_running_event_info(cpuid);
    let mut pid = event.pid;
    // for testing we hardcode to one for now
    pid = 1;
    let process_table = PROCESS_TABLE.write();
    let process = process_table
        .get(&pid)
        .expect("Could not get pcb from process table");
    let pcb = process.pcb.get();
    let mut begin_addr = unsafe { MMAP_ADDR };
    let mut mmap_call = MmapCall::new(begin_addr, begin_addr + len, fd, offset);

    if begin_addr + offset > (*HHDM_OFFSET).as_u64() {
        serial_println!("Ran out of virtual memory for mmap call.");
        return Some(*HHDM_OFFSET);
    }
    if flags == MmapFlags::MAP_ANONYMOUS {
        map_memory(&mut begin_addr, len, prot, &mut mmap_call);
    } else {
        allocate_file_memory(&mut begin_addr, len, fd, prot, offset, &mut mmap_call);
    }
    unsafe {
        (*pcb).mmaps.push(mmap_call);
    }

    Some(VirtAddr::new(begin_addr))
}

fn map_memory(begin_addr: &mut u64, len: u64, prot: u64, mmap_call: &mut MmapCall) {
    let page_count = len / PAGE_SIZE as u64;
    for _ in 0..page_count {
        let page: Page<Size4KiB> = Page::containing_address(VirtAddr::new(*begin_addr));
        let mut mapper = MAPPER.lock();
        let flags = protection_to_pagetable_flags(prot);
        create_not_present_mapping(page, &mut *mapper, Some(flags));
        serial_println!("MAPPING MEMORY PAGE {:?}", page);
        mmap_call.loaded.push(false);
        *begin_addr += PAGE_SIZE as u64;
    }

    mmap_call.fd = -1;
    mmap_call.offset = 0;
}

fn allocate_file_memory(
    begin_addr: &mut u64,
    len: u64,
    fd: i64,
    prot: u64,
    offset: u64,
    mmap_call: &mut MmapCall,
) {
    let page_count = len / PAGE_SIZE as u64;
    for _ in 0..page_count {
        let page: Page<Size4KiB> = Page::containing_address(VirtAddr::new(*begin_addr));
        let mut mapper = MAPPER.lock();
        let flags = protection_to_pagetable_flags(prot);
        create_not_present_mapping(page, &mut *mapper, Some(flags));
        *begin_addr += PAGE_SIZE as u64;
    }

    mmap_call.fd = fd;
    mmap_call.offset = offset;
}

#[cfg(test)]
mod tests {
    use crate::{
        constants::{
            memory::PAGE_SIZE,
            processes::{SYSCALL_BINARY, SYSCALL_MMAP_MEMORY},
        },
        events::schedule_process,
        processes::process::{create_process, run_process_ring3},
        serial_println,
        syscalls::mmap::MMAP_ADDR,
    };

    use super::{sys_mmap, MmapFlags, ProtFlags};

    #[test_case]
    fn test_noprocess_basic_anon_mmap() {
        let ret = sys_mmap(
            0x0,
            0x1000,
            ProtFlags::PROT_WRITE,
            MmapFlags::MAP_ANONYMOUS,
            -1,
            0,
        )
        .expect("Mmap failed");

        create_process(SYSCALL_BINARY);

        // make sure it gave us correct virtual address
        unsafe { serial_println!("RET: {:?}", MMAP_ADDR - PAGE_SIZE as u64) };
        unsafe { assert_eq!(ret.as_u64(), MMAP_ADDR - PAGE_SIZE as u64) };

        // write to the address and make sure we can
        unsafe { ret.as_mut_ptr::<u64>().write_volatile(0xdeadbeef) };

        let written_val = unsafe { ret.as_ptr::<u64>().read_volatile() };

        assert_eq!(written_val, 0xdeadbeef);
    }

    #[test_case]
    fn test_noprocess_basic_anon_mmap_2() {
        let ret = sys_mmap(
            0x0,
            0x2000,
            ProtFlags::PROT_WRITE,
            MmapFlags::MAP_ANONYMOUS,
            -1,
            0,
        )
        .expect("Mmap failed");

        create_process(SYSCALL_BINARY);

        // make sure it gave us correct virtual address
        unsafe { assert_eq!(ret.as_u64(), MMAP_ADDR - 2 * PAGE_SIZE as u64) };

        // write to the address and make sure we can
        unsafe { ret.as_mut_ptr::<u64>().write_volatile(0xdeadbeef) };

        let written_val = unsafe { ret.as_ptr::<u64>().read_volatile() };

        assert_eq!(written_val, 0xdeadbeef);
    }

    #[test_case]
    fn test_basic_anon_mmap() {
        let pid = create_process(SYSCALL_MMAP_MEMORY);
        unsafe { schedule_process(0, run_process_ring3(pid), pid) };
    }
}
