use crate::memory::bitmap_frame_allocator::BitmapFrameAllocator;
use crate::memory::boot_frame_allocator::BootIntoFrameAllocator;
use spin::{Mutex, MutexGuard};

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
    with_generic_allocator(|allocator| allocator.allocate_frame())
}

/// Exposed function to allocate a frame that runs the global's deallocate_frame
pub fn dealloc_frame(frame: PhysFrame<Size4KiB>) {
    with_generic_allocator(|allocator| unsafe { allocator.deallocate_frame(frame) })
}

pub fn with_bitmap_frame_allocator<F, R>(f: F) -> R
where
    F: FnOnce(&mut BitmapFrameAllocator) -> R,
{
    let mut guard = FRAME_ALLOCATOR.lock();
    let alloc = match &mut *guard {
        Some(GlobalFrameAllocator::Bitmap(alloc)) => alloc,
        _ => panic!("Allocator is not a BitmapFrameAllocator"),
    };
    f(alloc)
}

pub fn with_boot_into_frame_allocator<F, R>(f: F) -> R
where
    F: FnOnce(&mut BootIntoFrameAllocator) -> R,
{
    let mut guard = FRAME_ALLOCATOR.lock();
    let alloc = match &mut *guard {
        Some(GlobalFrameAllocator::Boot(alloc)) => alloc,
        _ => panic!("Allocator is not a BootIntoFrameAllocator"),
    };
    f(alloc)
}

pub fn with_generic_allocator<F, R>(f: F) -> R
where
    F: FnOnce(&mut GlobalFrameAllocator) -> R,
{
    let mut guard = FRAME_ALLOCATOR.lock();
    if let Some(ref mut allocator) = *guard {
        f(allocator)
    } else {
        panic!("Allocator does not exist.");
    }
}
