use crate::serial_println;
use limine::request::FramebufferRequest;
use pci::walk_pci_bus;
use sd_card::{find_sd_card, initalize_sd_card};
use x86_64::structures::paging::OffsetPageTable;
pub mod pci;
pub mod sd_card;
pub mod serial;

// TODO: Fix when we have a proper frame buffer
#[used]
#[link_section = ".requests"]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

pub fn init(cpu_id: u32, mapper: &mut OffsetPageTable) {
    if cpu_id == 0 {
        if let Some(framebuffer_response) = FRAMEBUFFER_REQUEST.get_response() {
            serial_println!("Found frame buffer");
            if let Some(framebuffer) = framebuffer_response.framebuffers().next() {
                for i in 0..100_u64 {
                    let pixel_offset = i * framebuffer.pitch() + i * 4;
                    unsafe {
                        *(framebuffer.addr().add(pixel_offset as usize) as *mut u32) = 0xFFFFFFFF;
                    }
                }
            }
        }
        let devices = walk_pci_bus();
        let sd_card_device =
            find_sd_card(&devices).expect("Build system currently sets up an sd-card");
        initalize_sd_card(&sd_card_device, mapper).unwrap();
    }
}
