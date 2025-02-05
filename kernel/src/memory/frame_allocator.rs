use limine::memory_map::EntryType;
use limine::response::MemoryMapResponse;
use x86_64::{
    structures::paging::{FrameAllocator, PhysFrame, Size4KiB},
    PhysAddr,
};

pub struct BootIntoFrameAllocator {
    memory_map: &'static MemoryMapResponse,
    next: usize,
}

impl BootIntoFrameAllocator {
    pub unsafe fn init(memory_map: &'static MemoryMapResponse) -> Self {
        BootIntoFrameAllocator {
            memory_map,
            next: 0,
        }
    }

    /// scan memory map and map only frames we know we can use
    pub fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        let regions = self.memory_map.entries().iter();
        // for region in regions.clone() {
        //     debug_println!("Region base 0x{:X}, length 0x{:X}, status {:?}", region.base, region.length, region.entry_type);
        // }
        let usable_regions = regions.filter(|r| r.entry_type == EntryType::USABLE);
        let addr_ranges = usable_regions.map(|r| r.base..=r.base + r.length);
        let frame_addresses = addr_ranges.flat_map(|r| r.step_by(4096));
        frame_addresses.map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }

    /// gets the frame of a specific physical memory access
    pub fn get_frame(&mut self, addr: u64) -> PhysFrame {
        PhysFrame::containing_address(PhysAddr::new(addr))
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootIntoFrameAllocator {
    /// allocates a frame that we can use
    /// FIXME: Does not work when next sees no more frames
    /// FIXME: Is not efficient
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}
