use crate::memory::{
    bitmap_frame_allocator::BitmapFrameAllocator, boot_frame_allocator::BootIntoFrameAllocator,
};
use spin::Mutex;

use x86_64::structures::paging::{FrameAllocator, FrameDeallocator, PhysFrame, Size4KiB};

/// Global frame allocator that makes it so we just have one actual allocator throughout codebase
/// Requires some basic synchronization
pub static FRAME_ALLOCATOR: Mutex<Option<GlobalFrameAllocator>> = Mutex::new(None);

/// Enum of supported allocators
pub enum GlobalFrameAllocator {
    Boot(BootIntoFrameAllocator),
    Bitmap(BitmapFrameAllocator),
}

unsafe impl FrameAllocator<Size4KiB> for GlobalFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        match self {
            GlobalFrameAllocator::Boot(ref mut boot_alloc) => boot_alloc.allocate_frame(),
            GlobalFrameAllocator::Bitmap(ref mut bitmap_alloc) => bitmap_alloc.allocate_frame(),
        }
    }
}

impl FrameDeallocator<Size4KiB> for GlobalFrameAllocator {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        match self {
            GlobalFrameAllocator::Boot(ref mut boot_alloc) => boot_alloc.deallocate_frame(frame),
            GlobalFrameAllocator::Bitmap(ref mut bitmap_alloc) => {
                bitmap_alloc.deallocate_frame(frame)
            }
        }
    }
}

/// Exposed function to allocate a frame that runs the global's allocate_frame
pub fn alloc_frame() -> Option<PhysFrame> {
    // Lock the global allocator.
    let mut allocator_lock = FRAME_ALLOCATOR.lock();

    // Get a mutable reference to the allocator if it exists, then call allocate_frame.
    if let Some(ref mut allocator) = *allocator_lock {
        allocator.allocate_frame()
    } else {
        None
    }
}

/// Exposed function to allocate a frame that runs the global's deallocate_frame
pub fn dealloc_frame(frame: PhysFrame<Size4KiB>) {
    // Lock the global allocator.
    let mut allocator_lock = FRAME_ALLOCATOR.lock();

    // Get a mutable reference to the allocator if it exists, then call allocate_frame.
    if let Some(ref mut deallocator) = *allocator_lock {
        unsafe { deallocator.deallocate_frame(frame) }
    }
}
