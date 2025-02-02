pub const PAGE_SIZE: usize = 4096;
pub const FRAME_SIZE: usize = 4096;
pub const HEAP_START: *mut u8 = 0x_0000_4444_4444_0000 as *mut u8;
pub const HEAP_SIZE: usize = 1024 * 1024; // 1 MB
// Max size of the heap is 256 frames, plus padding for any other allocations
pub const MAX_ALLOCATED_FRAMES: usize = 512;
