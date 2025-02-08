use crate::{constants::processes::BINARY, memory::paging::create_mapping};
use x86_64::{
    structures::paging::Page,
    structures::paging::{Mapper, Size4KiB},
    VirtAddr,
};

pub fn load_binary(
    kernel_end: u64,
    mapper: &mut impl Mapper<Size4KiB>, // Add mapper parameter
) -> *mut u8 {
    let binary_data = BINARY;
    let dest_ptr = kernel_end as *mut u8;

    let dest_start = VirtAddr::new(kernel_end);
    let dest_end = dest_start + binary_data.len() as u64;
    let start_page = Page::containing_address(dest_start);
    let end_page = Page::containing_address(dest_end);
    let page_range = Page::range_inclusive(start_page, end_page);

    // Map all pages in destination range
    for page in page_range {
        create_mapping(page, mapper);
    }

    unsafe {
        core::ptr::copy_nonoverlapping(binary_data.as_ptr(), dest_ptr, binary_data.len());
    }

    dest_ptr
}
