use crate::{
    constants::memory::{FRAME_SIZE, MAX_ALLOCATED_FRAMES},
    serial_println,
};
use limine::memory_map::EntryType;
use limine::response::MemoryMapResponse;
use x86_64::{
    structures::paging::{frame, FrameAllocator, FrameDeallocator, PhysFrame, Size4KiB},
    PhysAddr,
};

extern crate alloc;
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use super::paging::init;


pub struct BitmapFrameAllocator {
    total_frames: usize,
    bitmap: Box<[u64]>,
    to_allocate: u64,
}

impl BitmapFrameAllocator {
    pub unsafe fn init(
        memory_map: &'static MemoryMapResponse,
        initial_frames: impl Iterator<Item = PhysFrame>
    ) -> Self {

        let initial_frames_vec: Vec<PhysFrame> = initial_frames.collect();

        // get the total number of frames
        let mut true_end = 0u64;
        for entry in memory_map.entries().iter() {
            if entry.entry_type == EntryType::USABLE {
                let end_addr = entry.base + entry.length;
                if end_addr > true_end {
                    true_end = end_addr;
                }
            }
        }

        serial_println!("The top of physmem is: {}", {true_end});

        let total_frames = (true_end as usize + FRAME_SIZE - 1) / FRAME_SIZE;

        let bitmap_size = (total_frames + 63) / 64;

        serial_println!("The bitmap size in bytes is: {}", { bitmap_size });

        let bitmap = vec![0xFFFFFFFFFFFFFFFF; bitmap_size].into_boxed_slice();

        let mut allocator = Self {
            total_frames,
            bitmap,
            to_allocate: initial_frames_vec.len() as u64
        };

        for entry in memory_map.entries().iter() {
            if entry.entry_type == EntryType::USABLE {
                allocator.free_region(entry.base, entry.length);
            }
        }

        for frame in initial_frames_vec {
            allocator.mark_frame_used(frame);
        }
        
        allocator
    }

    /// Mark the region [base, base + length) as free in the bitmap.
    fn free_region(&mut self, base: u64, length: u64) {
        let start_frame = (base / FRAME_SIZE as u64) as usize;
        let end_frame = ((base + length) / FRAME_SIZE as u64) as usize;
        for frame_index in start_frame..end_frame {
            self.clear_bit(frame_index); // set to 0 = free
        }
    }

    /// Mark a specific frame as used (1).
    pub fn is_frame_used(&mut self, frame: PhysFrame) -> bool {
        let frame_index = (frame.start_address().as_u64() / FRAME_SIZE as u64) as usize;
        self.is_bit_set(frame_index)
    }

    /// Mark a specific frame as used (1).
    fn mark_frame_used(&mut self, frame: PhysFrame) {
        let frame_index = (frame.start_address().as_u64() / FRAME_SIZE as u64) as usize;
        self.set_bit(frame_index);
    }

    /// Mark a specific frame as free (0).
    fn mark_frame_free(&mut self, frame: PhysFrame) {
        let frame_index = (frame.start_address().as_u64() / FRAME_SIZE as u64) as usize;
        self.clear_bit(frame_index);
    }

    fn set_bit(&mut self, frame_index: usize) {
        assert!(frame_index < self.total_frames);

        let byte_index = frame_index / 64;
        let bit_index = frame_index % 64;

        let mask = 1 << bit_index;
        self.bitmap[byte_index] |= mask;
    }

    fn clear_bit(&mut self, frame_index: usize) {
        assert!(frame_index < self.total_frames);

        let byte_index = frame_index / 64;
        let bit_index = frame_index % 64;

        let mask = 1 << bit_index;
        self.bitmap[byte_index] &= !mask;
    }

    fn is_bit_set(&self, frame_index: usize) -> bool {
        assert!(frame_index < self.total_frames);

        let byte_index = frame_index / 64;
        let bit_index = frame_index % 64;

        let mask = 1 << bit_index;
        (self.bitmap[byte_index] & mask) != 0
    }
}

unsafe impl FrameAllocator<Size4KiB> for BitmapFrameAllocator {
    /// allocates a frame that we can use
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        loop {
            if !self.is_bit_set(self.to_allocate as usize) {
                self.set_bit(self.to_allocate as usize);
                let addr = self.to_allocate as u64 * FRAME_SIZE as u64;

                self.to_allocate = (self.to_allocate + 1) % self.total_frames as u64;

                return Some(PhysFrame::containing_address(PhysAddr::new(addr)));
            }

            self.to_allocate = (self.to_allocate + 1) % self.total_frames as u64;
        };
    }
}

impl FrameDeallocator<Size4KiB> for BitmapFrameAllocator {
    /// deallocates a frame
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        self.mark_frame_free(frame);
    }
}
