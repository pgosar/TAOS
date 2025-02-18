// This is only there because of get_page_table_entry() function being used in testcases,
// however it could be used in a plethora of places later so I am keeping it for now
#![allow(dead_code)]

use x86_64::{
    instructions::tlb,
    structures::paging::{
        Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB,
    },
    VirtAddr,
};

use crate::{
    constants::memory::EPHEMERAL_KERNEL_MAPPINGS_START,
    memory::{
        frame_allocator::{alloc_frame, dealloc_frame, FRAME_ALLOCATOR},
        HHDM_OFFSET,
    },
    constants::idt::TLB_SHOOTDOWN_VECTOR,
    interrupts::x2apic::X2ApicManager,
    memory::{frame_allocator::{alloc_frame, dealloc_frame, FRAME_ALLOCATOR}, tlb::tlb_shootdown},
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

    let _ = unsafe {
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
        ).expect("Mapping failed")
    };

    // tlb_shootdown(page.start_address());

    frame
}

/// updates an existing mapping
pub fn update_mapping(page: Page, mapper: &mut impl Mapper<Size4KiB>, frame: PhysFrame<Size4KiB>) {
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

    let old_frame = mapper
        .translate_page(page)
        .expect("No mapping currently exists");

    if old_frame != frame {
        let map_to_result = unsafe {
            mapper.map_to(
                page,
                frame,
                flags,
                FRAME_ALLOCATOR
                    .lock()
                    .as_mut()
                    .expect("Global allocator not initialized"),
            )
        };

        map_to_result.expect("Mapping failed").flush();
        X2ApicManager::send_ipi_all_cores(TLB_SHOOTDOWN_VECTOR).expect("Failed to flush TLBs");
    }
}

pub fn remove_mapping(page: Page, mapper: &mut impl Mapper<Size4KiB>) -> PhysFrame<Size4KiB> {
    let (frame, _) = mapper.unmap(page).expect("map_to failed");
    tlb_shootdown(page.start_address());
    frame
}

pub fn remove_mapped_frame(page: Page, mapper: &mut impl Mapper<Size4KiB>) {
    let (frame, _) = mapper.unmap(page).expect("map_to failed");
    dealloc_frame(frame);
    tlb_shootdown(page.start_address());
}

//update permissions for a specific page
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

/// Returns a reference to the page table entry for the given page.
///
/// # Safety
///
/// The caller must ensure that the provided `mapper` (an OffsetPageTable)
/// was initialized with the correct physical memory offset.
///
/// Needed because x86_64 crate does not expose a method to get PageTableEntry
unsafe fn get_page_table_entry<'a>(
    page: Page<Size4KiB>,
    mapper: &OffsetPageTable<'a>,
) -> Option<&'a x86_64::structures::paging::page_table::PageTableEntry> {
    // Calculate indices for the four levels.
    let p4_index = page.p4_index();
    let p3_index = page.p3_index();
    let p2_index = page.p2_index();
    let p1_index = page.p1_index();

    // Get a reference to the level 4 table.
    let level_4_table = mapper.level_4_table();

    // Walk down the page table hierarchy:
    let p4_entry = &level_4_table[p4_index];
    let p3_frame = p4_entry.frame().expect("Entry failed");
    let p3_table_ptr = (*HHDM_OFFSET + p3_frame.start_address().as_u64()).as_ptr::<PageTable>();
    let p3_table = &*p3_table_ptr;

    let p3_entry = &p3_table[p3_index];
    let p2_frame = p3_entry.frame().expect("Entry failed");
    let p2_table_ptr = (*HHDM_OFFSET + p2_frame.start_address().as_u64()).as_ptr::<PageTable>();
    let p2_table = &*p2_table_ptr;

    let p2_entry = &p2_table[p2_index];
    let p1_frame = p2_entry.frame().expect("Entry failed");
    let p1_table_ptr = (*HHDM_OFFSET + p1_frame.start_address().as_u64()).as_ptr::<PageTable>();
    let p1_table = &*p1_table_ptr;

    let pte = &p1_table[p1_index];
    Some(pte)
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

#[cfg(test)]
mod tests {
    use core::{
        future::Future,
        ptr::{read_volatile, write_volatile},
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;
    use crate::{
        constants::{memory::PAGE_SIZE, MAX_CORES},
        events::schedule,
        interrupts::x2apic::{current_core_id, TLB_SHOOTDOWN_ADDR},
        memory::{tlb, MAPPER},
        serial_print, serial_println,
    };
    use alloc::vec::Vec;
    use x86_64::structures::paging::mapper::TranslateError;

    // used for tlb shootdown testcases
    static PRE_READ: AtomicU64 = AtomicU64::new(0);
    static POST_READ: AtomicU64 = AtomicU64::new(0);

    #[test_case]
    fn test_remove_mapped_frame() {
        let mut mapper = MAPPER.lock();

        let page: Page = Page::containing_address(VirtAddr::new(0x500000000));
        let _ = create_mapping(page, &mut *mapper, None);

        remove_mapped_frame(page, &mut *mapper);

        let translate_frame_error = mapper.translate_page(page);

        assert!(matches!(
            translate_frame_error,
            Err(TranslateError::PageNotMapped)
        ));
    }

    #[test_case]
    fn test_basic_map_and_translate() {
        let mut mapper = MAPPER.lock();

        // random test virtual page
        let page: Page = Page::containing_address(VirtAddr::new(0x500000000));
        let frame: PhysFrame = create_mapping(page, &mut *mapper, None);

        let translate_frame = mapper.translate_page(page).expect("Translation failed");

        assert_eq!(frame, translate_frame);

        remove_mapped_frame(page, &mut *mapper);
    }

    #[test_case]
    fn test_update_permissions() {
        let mut mapper = MAPPER.lock();

        let page: Page = Page::containing_address(VirtAddr::new(0x500000000));
        let _ = create_mapping(page, &mut *mapper, None);

        let flags = PageTableFlags::PRESENT;

        unsafe { update_permissions(page, &mut *mapper, flags) };

        let pte = unsafe { get_page_table_entry(page, &mut *mapper) }.expect("Getting PTE Failed");

        assert!(pte.flags().contains(PageTableFlags::PRESENT));
        assert!(!pte.flags().contains(PageTableFlags::WRITABLE));

        remove_mapped_frame(page, &mut *mapper);
    }

    #[test_case]
    fn test_contiguous_mapping() {
        let mut mapper = MAPPER.lock();

        // Define a contiguous region spanning 8 pages.
        let start_page: Page = Page::containing_address(VirtAddr::new(0x500000000));
        let num_pages = 8;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        let mut frames = Vec::new();
        for i in 0..num_pages {
            let page = Page::from_start_address(start_page.start_address() + i * PAGE_SIZE as u64)
                .expect("Invalid page address");
            let frame = create_mapping(page, &mut *mapper, Some(flags));
            frames.push((page, frame));
        }

        // Write and verify distinct values.
        for (i, (page, _)) in frames.iter().enumerate() {
            let ptr = page.start_address().as_mut_ptr::<u64>();
            unsafe { write_volatile(ptr, i as u64) };
            let val = unsafe { read_volatile(ptr) };
            assert_eq!(val, i as u64,);
        }

        // Cleanup: Unmap all pages.
        for (page, _) in frames {
            remove_mapped_frame(page, &mut *mapper);
        }
    }

    #[test_case]
    fn test_tlb_shootdowns_basic() {
        let target_vaddr = VirtAddr::new(0x500000000);
        let current = current_core_id();

        tlb_shootdown(target_vaddr);

        let addresses = TLB_SHOOTDOWN_ADDR.lock();

        // Verify that every core except the current one got updated with the target address.
        for core in 0..MAX_CORES {
            if core != current {
                let stored = addresses[core];
                assert_eq!(stored, target_vaddr.as_u64(),);
            } else {
                // The current core should not store the address (it performs the invlpg directly).
                let stored = addresses[core];
                assert_eq!(stored, 0,);
            }
        }
    }

    // Goal: Create a mapping and access it on some core such that it is cached.
    // Then, change the mapping to map to a different frame such that a TLB Shootdown
    // is necessary.
    // Finally, check the mapping on another core.
    // #[test_case]
    // fn test_tlb_shootdowns_cross_core() -> impl Future<Output = ()> + Send + 'static{
    //     async move {
    //         let page: Page = Page::containing_address(VirtAddr::new(0x500000000));
    //     }
    //
    //     // create mapping and set value on current core to cache page
    //     let page: Page = Page::containing_address(VirtAddr::new(0x500000000));
    //
    //     {
    //         let mut mapper = MAPPER.lock();
    //         let _ = create_mapping(page, &mut *mapper, None);
    //         unsafe {
    //             page.start_address()
    //                 .as_mut_ptr::<u64>()
    //                 .write_volatile(0xdead);
    //         }
    //     }
    //
    //     // mapping exists now and is cached for first core
    //
    //     // tell core 1 to read the value (to cache) and wait until it's done
    //     schedule(
    //         1,
    //         async move {
    //             let value = unsafe { page.start_address().as_ptr::<u64>().read_volatile() };
    //             PRE_READ.store(value, Ordering::SeqCst);
    //         }
    //         .await,
    //         3,
    //         1,
    //     );
    //
    //     while PRE_READ.load(Ordering::SeqCst) == 0 {
    //         // busy wait
    //     }
    //
    //     serial_println!("Debug print");
    //
    //     {
    //         let mut mapper = MAPPER.lock();
    //         let new_frame = alloc_frame().expect("Could not find a new frame");
    //
    //         // could say page already mapped, which would be really dumb
    //         update_mapping(page, &mut *mapper, new_frame);
    //
    //         unsafe {
    //             page.start_address()
    //                 .as_mut_ptr::<u64>()
    //                 .write_volatile(0x42);
    //         }
    //     }
    //
    //     // back on core 1, read the value and see if it has changed
    //     schedule(
    //         1,
    //         async move {
    //             let value = unsafe { page.start_address().as_mut_ptr::<u64>().read_volatile() };
    //             POST_READ.store(value, Ordering::SeqCst);
    //         },
    //         0,
    //         2,
    //     );
    //
    //     while POST_READ.load(Ordering::SeqCst) == 0 {
    //         // busy wait
    //     }
    //
    //     assert_eq!(POST_READ.load(Ordering::SeqCst), 0x42);
    // }
}
