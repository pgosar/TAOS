use x86_64::{
    structures::paging::{OffsetPageTable, Page, Translate},
    VirtAddr,
};

use crate::{
    debug_println,
    devices::pci::{write_command, COMMAND_IO_SPACE, COMMAND_MEMORY_SPACE},
    memory::frame_allocator::BootIntoFrameAllocator,
    memory::paging,
};

use super::pci::{read_config, AllDeviceInfo, DeviceInfo};

const SD_CLASS_CODE: u8 = 0x8;
const SD_SUB_CLASS: u8 = 0x5;
const SD_NO_DMA_INTERFACE: u8 = 0x0;
const SD_DMA_INTERFACE: u8 = 0x1;
const SD_VENDOR_UNIQUE_INTERFACE: u8 = 0x2;

/// Finds the device that represents an SD card. It must support DMA
pub fn find_sd_card(devices: &AllDeviceInfo) -> Option<&DeviceInfo> {
    for possible_device in &devices.device_info {
        match possible_device {
            None => (),
            Some(device) => {
                if device.class_code == SD_CLASS_CODE
                    && device.subclass == SD_SUB_CLASS
                    && device.programming_interface == SD_DMA_INTERFACE
                {
                    return Option::Some(device);
                }
            }
        }
    }
    return Option::None;
}

pub fn initalize_sd_card(
    sd_card: &DeviceInfo,
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut BootIntoFrameAllocator,
) {
    // We need to determine the number of slots 
    let slot_info_register = unsafe { read_config(sd_card.bus, sd_card.device, 0, 0x40) };
    let first_base_addr_reg = slot_info_register & 0x7;
    debug_println!("First bar is 0x{:X}", first_base_addr_reg);


    let command = sd_card.command & !(COMMAND_IO_SPACE | COMMAND_MEMORY_SPACE);
    unsafe {
        write_command(sd_card.bus, sd_card.device, 0, command);
    }
    let base_address_register = unsafe { read_config(sd_card.bus, sd_card.device, 0, 0x10) };
    debug_println!("Bar space indicator is 0x{:X}", base_address_register & 0x1);
    debug_println!(
        "Bar IO base addr is 0x{:X}",
        base_address_register & 0xFFFFFF00
    );
    let bar_address: u64 = (base_address_register & 0xFFFFFF00).into();

    unsafe {
        write_command(sd_card.bus, sd_card.device, 0, sd_card.command);
    }

    let page = Page::containing_address(VirtAddr::new(bar_address));

    paging::create_uncachable_mapping(page, mapper, frame_allocator);
    let phys = mapper.translate_addr(VirtAddr::new(bar_address));

    debug_println!(
        "{:?} -> {:?}",
        VirtAddr::new((base_address_register & 0xFFFFFF00).into()),
        phys
    );


}
