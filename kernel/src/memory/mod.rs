pub mod bitmap_frame_allocator;
pub mod boot_frame_allocator;
pub mod frame_allocator;
pub mod heap;
pub mod paging;

use boot_frame_allocator::BootIntoFrameAllocator;
use frame_allocator::{GlobalFrameAllocator, FRAME_ALLOCATOR};
use x86_64::structures::paging::OffsetPageTable;

pub fn init(cpu_id: u32)  -> Option<OffsetPageTable<'static>> {
    if cpu_id == 0 {
        unsafe {
            *FRAME_ALLOCATOR.lock() =
                Some(GlobalFrameAllocator::Boot(BootIntoFrameAllocator::init()));
        }
        let mut mapper = unsafe { paging::init() };
        heap::init_heap(&mut mapper).expect("Failed to initialize heap");
        return Option::Some(mapper)
    }
    Option::None
}
