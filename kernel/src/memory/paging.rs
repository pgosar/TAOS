use x86_64::{
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame,
        Size4KiB,
    },
    VirtAddr,
};

use crate::debug_println;

/// initializes vmem system. activates pml4 and sets up page tables
pub unsafe fn init(hhdm_base: VirtAddr) -> OffsetPageTable<'static> {
    debug_println!("HHdm base: 0x{:X}", hhdm_base.as_u64());
    let pml4 = active_level_4_table(hhdm_base);

    OffsetPageTable::new(pml4, hhdm_base)
}

/// activates pml4
unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    &mut *page_table_ptr
}

/// Creates an example mapping, in unsafe
pub fn create_mapping(
    page: Page,
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) {
    use x86_64::structures::paging::PageTableFlags as Flags;

    let default_flags = Flags::PRESENT | Flags::WRITABLE;
    create_mapping_with_flags(page, mapper, frame_allocator, default_flags);
}

/// Creates a example mapping, in unsafe. Additonally mapps said mapping
/// To exist in uncacheable memory, making it usefull for memory mapped IO
pub fn create_uncachable_mapping_given_frame(
    page: Page,
    mapper: &mut OffsetPageTable,
    frame: PhysFrame,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) {
    use x86_64::structures::paging::PageTableFlags as Flags;

    // Additionally this should set up this page to map to PAT 3,
    // Which is by default set to Strong Uncacheable. See Section
    // 13.12 of Volume 3 of the Intel Manual (December 2024)
    let uncacheable_flags =
        Flags::PRESENT | Flags::WRITABLE | Flags::WRITE_THROUGH | Flags::NO_CACHE;

    let map_to_result = unsafe {
        // fixme: this is not safe, we do it only for testing
        mapper.map_to(page, frame, uncacheable_flags, frame_allocator)
    };
    map_to_result.expect("map_to failed").flush();
}

fn create_mapping_with_flags(
    page: Page,
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    flags: PageTableFlags,
) {
    let frame = frame_allocator.allocate_frame().expect("no more frames");

    let map_to_result = unsafe {
        // fixme: this is not safe, we do it only for testing
        mapper.map_to(page, frame, flags, frame_allocator)
    };
    map_to_result.expect("map_to failed").flush();
}
