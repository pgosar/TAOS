pub const PAGE_SIZE: u64 = 4096;
pub const FRAME_SIZE: usize = 4096;

// Changed the first non F character of kernel_end to an F
pub const HEAP_START: *mut u8 = 0xFFFF_FFFF_0000_0000 as *mut u8;
// Max size of the heap is 256 frames, plus padding for any other allocations
pub const HEAP_SIZE: usize = 1024 * 1024; // 1 MB

pub const MAX_ALLOCATED_FRAMES: usize = 512;
pub const BITMAP_ENTRY_SIZE: usize = 64;
pub const FULL_BITMAP_ENTRY: u64 = 0xFFFFFFFFFFFFFFFF;

pub const EPHEMERAL_KERNEL_MAPPINGS_START: u64 = 0xFFFF_FF80_0000_0000;
