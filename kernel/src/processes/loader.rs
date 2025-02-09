use crate::{constants::processes::BINARY, memory::paging::create_mapping, serial_println};
use x86_64::{
    structures::paging::Page,
    structures::paging::{Mapper, Size4KiB},
    VirtAddr,
};

pub fn load_binary(hhdm_offset: VirtAddr, mapper: &mut impl Mapper<Size4KiB>) {
    let binary_data = BINARY;
    let dest_ptr: *mut u8 = 0x124000 as *mut u8;
    let dest_end = hhdm_offset;
    let binary_size = binary_data.len() as u64;

    let dest_start = VirtAddr::new_truncate(0x124000);
    let start_page: Page = Page::containing_address(dest_start);
    let end_page = Page::containing_address(dest_end);
    let page_range = Page::range_inclusive(start_page, end_page);
    serial_println!("HELLo");

    use x86_64::registers::control::Cr3;
    serial_println!("CR 3 {:#?}", Cr3::read());
    create_mapping(start_page, mapper);
    for page in page_range {
        serial_println!("HELLO");
        serial_println!("{:#?}", page);
        create_mapping(page, mapper);
    }

    unsafe {
        core::ptr::copy_nonoverlapping(binary_data.as_ptr(), dest_ptr, binary_data.len());
    }
    unsafe {
        let entry: extern "C" fn() = core::mem::transmute(dest_ptr);
        //entry();
    }
}
