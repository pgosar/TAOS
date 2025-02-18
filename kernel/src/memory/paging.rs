use lazy_static::lazy_static;
use x86_64::{
    structures::paging::{
        mapper::CleanUp, FrameDeallocator, Mapper, OffsetPageTable, Page, PageTable,
        PageTableFlags, PhysFrame, Size4KiB,
    },
    VirtAddr,
};

use crate::{constants::memory::EPHEMERAL_KERNEL_MAPPINGS_START, serial_println};
use crate::memory::{
    frame_allocator::{alloc_frame, dealloc_frame, FRAME_ALLOCATOR},
    HHDM_OFFSET,
};

use super::bitmap_frame_allocator::BitmapFrameAllocator;

static mut NEXT_EPH_OFFSET: u64 = 0;

/// initializes vmem system. activates pml4 and sets up page tables
///
/// # Safety
///
/// TODO
pub unsafe fn init() -> OffsetPageTable<'static> {
    OffsetPageTable::new(active_level_4_table(*HHDM_OFFSET), *HHDM_OFFSET)
}

/// activates pml4
/// # Safety
///
/// TODO
pub unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    &mut *page_table_ptr
}

/// Creates an example mapping, is unsafe
/// Takes in page, mapper, and nullable flags (default flags)
/// Default flags: PRESENT | WRITABLE
pub fn create_mapping(
    page: Page,
    mapper: &mut impl Mapper<Size4KiB>,
    flags: Option<PageTableFlags>,
) -> PhysFrame {
    let frame = alloc_frame().expect("no more frames");

    let map_to_result = unsafe {
        // FIXME: this is not safe, we do it only for testing
        mapper.map_to(
            page,
            frame,
            flags.unwrap_or(
                PageTableFlags::PRESENT
                    | PageTableFlags::WRITABLE
                    | PageTableFlags::USER_ACCESSIBLE,
            ),
            FRAME_ALLOCATOR
                .lock()
                .as_mut()
                .expect("Global allocator not initialized"),
        )
    };

    // serial_println!("Allocated page with flags: {}", flags.unwrap_or(PageTableFlags::PRESENT | PageTableFlags::WRITABLE).bits());
    map_to_result.expect("map_to failed").flush();
    frame
}

pub fn remove_mapping(page: Page, mapper: &mut impl Mapper<Size4KiB>) {
    let (frame, flush) = mapper.unmap(page).expect("map_to failed");
    dealloc_frame(frame);
    flush.flush();
}

pub fn map_kernel_frame(
    mapper: &mut impl Mapper<Size4KiB>,
    frame: PhysFrame,
    flags: PageTableFlags,
) -> VirtAddr {
    let offset = unsafe {
        let current = NEXT_EPH_OFFSET;
        NEXT_EPH_OFFSET += 0x1000; // move up by a page
        current
    };

    let temp_virt = VirtAddr::new(EPHEMERAL_KERNEL_MAPPINGS_START + offset);
    let temp_page = Page::containing_address(temp_virt);

    unsafe {
        let result = mapper.map_to(
            temp_page,
            frame,
            flags,
            FRAME_ALLOCATOR
                .lock()
                .as_mut()
                .expect("Global allocator not initialized"),
        );
        result.expect("Map To Failed").flush();
    }

    temp_virt
}

///update permissions for a specific page
/// # Safety
///
/// TODO
pub unsafe fn update_permissions(
    page: Page,
    mapper: &mut impl Mapper<Size4KiB>,
    flags: PageTableFlags,
) {
    mapper
        .update_flags(page, flags)
        .expect("Updating flags failed")
        .flush();
    // TODO: Deal with TLB Shootdowns
}

pub fn clear_process_frames(pml4_frame: PhysFrame<Size4KiB>) {
    let mut allocator_lock = FRAME_ALLOCATOR.lock();

    let phys_addr = pml4_frame.start_address();
    let virt_addr = *HHDM_OFFSET + phys_addr.as_u64();
    let pml4_ptr: *mut PageTable = virt_addr.as_mut_ptr();
   let mapper = unsafe{
    OffsetPageTable::new(&mut *pml4_ptr, *HHDM_OFFSET)
    };
    // for (i, entry) in mapper.level_4_table().iter().enumerate() {
    //     serial_println!("{:?}", entry);
    // }

    // clear empty p1-p3 tables
    // if let Some(ref mut deallocator) = *allocator_lock {
    //     unsafe {
    //         mapper.clean_up(deallocator);
    //     }
    // }

    

    // let m = *mapper;
    // m.clean_up(deallocator);
}
