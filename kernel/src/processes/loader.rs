use crate::{
    constants::processes::{STACK_SIZE, STACK_START},
    memory::paging::create_mapping,
    serial_println,
};
use core::arch::asm;
use goblin::{
    elf::Elf,
    elf64::program_header::{PF_W, PF_X, PT_LOAD},
};
use x86_64::{
    structures::paging::{Mapper, Size4KiB},
    structures::paging::{Page, PageTableFlags},
    VirtAddr,
};

pub fn load_elf(elf_bytes: &[u8], mapper: &mut impl Mapper<Size4KiB>) -> VirtAddr {
    let elf = Elf::parse(elf_bytes).expect("Parsing ELF failed");
    serial_println!(
        "ELF parsed successfully. Entry point: 0x{:x}",
        elf.header.e_entry
    );

    // PT_LOAD segments
    for (i, ph) in elf.program_headers.iter().enumerate() {
        if ph.p_type != PT_LOAD {
            continue;
        }
        let virt_addr = VirtAddr::new(ph.p_vaddr);
        let mem_size = ph.p_memsz as usize;
        let file_size = ph.p_filesz as usize;
        let offset = ph.p_offset as usize;

        serial_println!(
            "Mapping segment {}: vaddr=0x{:x}, mem_size={}, file_size={}, offset=0x{:x}",
            i,
            ph.p_vaddr,
            mem_size,
            file_size,
            offset
        );

        let start_page: Page<Size4KiB> = Page::containing_address(virt_addr);
        let end_page: Page<Size4KiB> =
            Page::containing_address(VirtAddr::new(ph.p_vaddr + mem_size as u64 - 1));

        let mut flags = PageTableFlags::PRESENT;
        if (ph.p_flags & PF_W) != 0 {
            flags |= PageTableFlags::WRITABLE;
        }
        if (ph.p_flags & PF_X) == 0 {
            flags |= PageTableFlags::NO_EXECUTE;
        }

        for page in Page::range_inclusive(start_page, end_page) {
            create_mapping(page, mapper, None);
        }

        unsafe {
            // Where segment is in virtual memory
            let dest = ph.p_vaddr as *mut u8;
            let src = &elf_bytes[offset..offset + file_size];
            core::ptr::copy_nonoverlapping(src.as_ptr(), dest, file_size);

            // BSS section should be zeroed out
            if mem_size > file_size {
                let bss_start = dest.add(file_size);
                core::ptr::write_bytes(bss_start, 0, mem_size - file_size);
            }
        }

        // Now that the page is written, we can update the flags
        for page in Page::range_inclusive(start_page, end_page) {
            unsafe {
                mapper
                    .update_flags(page, flags)
                    .expect("Updating flags failed")
                    .flush();
            }
        }

        serial_println!("Segment {} loaded successfully.", i);
    }

    let stack_start = VirtAddr::new(STACK_START);
    let stack_end = VirtAddr::new(STACK_START + STACK_SIZE as u64);
    let start_page: Page<Size4KiB> = Page::containing_address(stack_start);
    let end_page: Page<Size4KiB> = Page::containing_address(stack_end);

    serial_println!(
        "Mapping stack: start=0x{:x}, end=0x{:x} ({} pages)",
        stack_start.as_u64(),
        stack_end.as_u64(),
        STACK_SIZE / 4096
    );

    for page in Page::range_inclusive(start_page, end_page) {
        create_mapping(page, mapper, None);
    }

    serial_println!(
        "Stack mapped successfully. Initial SP (stack pointer): 0x{:x}",
        stack_end.as_u64()
    );
    //unsafe {
    //    asm!(
    //        "mov rsp, {}",
    //        "jmp {}",
    //        in(reg) stack_end.as_u64(),
    //        in(reg) elf.header.e_entry,
    //        options(noreturn)
    //    );
    //}
    stack_end
}
