use x86_64::{
    structures::paging::{
        FrameDeallocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame,
        Size4KiB,
    },
    VirtAddr,
};

use crate::{
    constants::memory::EPHEMERAL_KERNEL_MAPPINGS_START,
    memory::frame_allocator::with_generic_allocator, processes::process::PCB, serial_println,
};

use super::{
    frame_allocator::{alloc_frame, dealloc_frame, FRAME_ALLOCATOR},
    HHDM_OFFSET,
};

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

/// Clear the PML4 associated with the PCB
///
/// * `pcb`: The process PCB to clear memory for
pub fn clear_process_frames(pcb: &mut PCB) {
    // let mut allocator_lock = FRAME_ALLOCATOR.lock();
    // let deallocator = match &mut *allocator_lock {
    //     Some(deallocator) => deallocator,
    //     None => {
    //         panic!("No deallocator found to clear process frames")
    //     }
    // };

    // let pml4_frame = pcb.pml4_frame;
    let mapper = unsafe { pcb.create_mapper() };

    with_generic_allocator(|allocator| {
        for i in 0..256 {
            let addr = mapper.level_4_table()[i].addr();
            let unused = mapper.level_4_table()[i].is_unused();
            if unused {
                continue;
            }
            let pdpt_frame = PhysFrame::containing_address(addr);
            unsafe {
                free_page_table(pdpt_frame, 3, allocator, HHDM_OFFSET.as_u64());
            }
        }
    });

    serial_println!("Process memory cleared");
}

/// Recursively deallocates a page table
///
/// # Safety
/// TODO
unsafe fn free_page_table(
    frame: PhysFrame,
    level: u8,
    deallocator: &mut impl FrameDeallocator<Size4KiB>,
    hhdm_offset: u64,
) {
    let virt = hhdm_offset + frame.start_address().as_u64();
    let table = unsafe { &mut *(virt as *mut PageTable) };

    for entry in table.iter_mut() {
        if entry.is_unused() {
            continue;
        }

        if level > 1 {
            let child_frame = PhysFrame::containing_address(entry.addr());
            free_page_table(child_frame, level - 1, deallocator, hhdm_offset);
        } else {
            // Free level one page
            let page_frame = PhysFrame::containing_address(entry.addr());
            deallocator.deallocate_frame(page_frame);
        }
    }
}
