use limine::memory_map::{Entry, EntryType};
use limine::request::*;
use limine::response::MemoryMapResponse;
use x86_64::structures::paging::{FrameAllocator, PhysFrame, Size4KiB};
use x86_64::{PhysAddr, VirtAddr};

pub struct BootIntoFrameAllocator {
    memory_map: &'static MemoryMapResponse,
    next: usize,
}

impl BootIntoFrameAllocator {
    pub unsafe fn init(memory_map: &'static MemoryMapResponse) -> Self {
        return BootIntoFrameAllocator {
            memory_map,
            next: 0,
        }
    }

    pub fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        let regions = self.memory_map.entries().iter();
        let usable_regions = regions.filter(|r| r.entry_type == EntryType::USABLE);
        let addr_ranges = usable_regions.map(|r| r.base..r.base + r.length + 1);
        let frame_addresses = addr_ranges.flat_map(|r| r.step_by(4096));
        frame_addresses.map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }

    pub fn get_frame(&mut self, addr: u64) -> PhysFrame {
        let frame = PhysFrame::containing_address(PhysAddr::new(addr));
        frame
    }

}

unsafe impl FrameAllocator<Size4KiB> for BootIntoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}
