use x86_64::{
    structures::paging::{
        mapper::{MappedFrame, TranslateResult},
        Mapper, OffsetPageTable, Page, PageTableFlags, PhysFrame, Size1GiB, Size2MiB, Size4KiB,
        Translate,
    },
    PhysAddr, VirtAddr,
};

use crate::memory::paging;

/// An error occured when setting up a frame as uncacheable
pub struct MMIOError;

/// Maps the requested_phys_addr as an uncacheable page above hhdm offset
pub fn map_page_as_uncacheable(
    requested_phys_addr: u64,
    mapper: &mut OffsetPageTable,
) -> Result<u64, MMIOError> {
    let offset = mapper.phys_offset().as_u64();
    let mut offset_bar = requested_phys_addr + offset;
    let translate_result = mapper.translate(VirtAddr::new(offset_bar));
    match translate_result {
        TranslateResult::Mapped {
            frame,
            offset: _,
            flags,
        } => match frame {
            MappedFrame::Size4KiB(_) => {
                let page: Page<Size4KiB> = Page::containing_address(VirtAddr::new(offset_bar));
                unsafe {
                    mapper
                        .update_flags(
                            page,
                            flags | PageTableFlags::NO_CACHE | PageTableFlags::WRITABLE,
                        )
                        .map_err(|_| MMIOError)?
                        .flush();
                }
            }
            MappedFrame::Size2MiB(_) => {
                let page: Page<Size2MiB> = Page::containing_address(VirtAddr::new(offset_bar));
                unsafe {
                    mapper
                        .update_flags(
                            page,
                            flags | PageTableFlags::NO_CACHE | PageTableFlags::WRITABLE,
                        )
                        .map_err(|_| MMIOError)?
                        .flush();
                }
            }
            MappedFrame::Size1GiB(_) => {
                let page: Page<Size1GiB> = Page::containing_address(VirtAddr::new(offset_bar));
                unsafe {
                    mapper
                        .update_flags(
                            page,
                            flags | PageTableFlags::NO_CACHE | PageTableFlags::WRITABLE,
                        )
                        .map_err(|_| MMIOError)?
                        .flush();
                }
            }
        },
        TranslateResult::InvalidFrameAddress(_) => {
            panic!("Invalid physical address in SD BAR")
        }
        TranslateResult::NotMapped => {
            let bar_frame: PhysFrame<Size4KiB> =
                PhysFrame::containing_address(PhysAddr::new(requested_phys_addr));
            let new_va = paging::map_kernel_frame(
                mapper,
                bar_frame,
                PageTableFlags::PRESENT | PageTableFlags::NO_CACHE | PageTableFlags::WRITABLE,
            );
            offset_bar = new_va.as_u64();
        }
    }
    Result::Ok(offset_bar)
}
