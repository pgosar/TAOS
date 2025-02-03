use x86_64::{
    structures::paging::{OffsetPageTable, Page, PhysFrame, Translate},
    PhysAddr, VirtAddr,
};

use crate::{
    debug_println,
    devices::pci::{
        write_pci_command, write_pci_data, COMMAND_BUS_MASTER, COMMAND_IO_SPACE,
        COMMAND_MEMORY_SPACE,
    },
    memory::{frame_allocator::BootIntoFrameAllocator, paging},
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
    // Lets assume 1 slot, and it uses BAR 1

    let command = sd_card.command & !(COMMAND_MEMORY_SPACE);
    unsafe {
        write_pci_command(sd_card.bus, sd_card.device, 0, command);
    }
    let base_address_register = unsafe { read_config(sd_card.bus, sd_card.device, 0, 0x10) };
    let bar_address: u64 = (base_address_register & 0xFFFFFF00).into();
    let bar_frame = PhysFrame::from_start_address(PhysAddr::new(bar_address)).unwrap();

    let page = Page::containing_address(VirtAddr::new(bar_address));
    paging::create_uncachable_mapping_given_frame(page, mapper, bar_frame, frame_allocator);
    unsafe {
        write_pci_command(
            sd_card.bus,
            sd_card.device,
            0,
            sd_card.command | COMMAND_MEMORY_SPACE,
        );
    }

    // Weve mapped stuff to physcial memory, mAYBE
}
