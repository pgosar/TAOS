use x86_64::{
    structures::paging::{FrameAllocator, Mapper, Page, OffsetPageTable, PageTable, PhysFrame, Size4KiB},
    VirtAddr,
    PhysAddr,
};

unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    &mut *page_table_ptr
}

pub unsafe fn init(hhdm_base: VirtAddr) -> OffsetPageTable<'static> {
    let pml4 = active_level_4_table(hhdm_base);

    OffsetPageTable::new(pml4, hhdm_base)
}

/// Creates an example mapping for the given page to frame `0xb8000`.
pub fn create_example_mapping(
    page: Page,
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) {
    use x86_64::structures::paging::PageTableFlags as Flags;

    let flags = Flags::PRESENT | Flags::WRITABLE;
    let frame = frame_allocator.allocate_frame().expect("no more frames");

    let map_to_result = unsafe {
        // FIXME: this is not safe, we do it only for testing
        mapper.map_to(page, frame, flags, frame_allocator)
    };
    map_to_result.expect("map_to failed").flush();
}
