use x86_64::{
    structures::paging::{OffsetPageTable, Page, PhysFrame},
    PhysAddr, VirtAddr,
};

use crate::{
    debug_println,
    devices::pci::{write_pci_command, COMMAND_MEMORY_SPACE},
    memory::{frame_allocator::BootIntoFrameAllocator, paging},
};
use bitflags::bitflags;

use super::pci::{read_config, AllDeviceInfo, DeviceInfo};

#[derive(Clone, Copy, Debug)]
pub struct SDCardInfo {
    device_info: DeviceInfo,
    cababilities: u64,
    base_address_register: u64,
    version: u8,
}

enum SDCardError {
    CommandInhibited,
    CommandStoppedDueToError,
    SDTimeout,
}

bitflags! {
    pub struct TransferModeFlags: u16 {
        const ResponseInterruptDisable = 1 << 8;
        const ResponseErrorChecKEnable = 1 << 7;
        const ResponseTypeSDIO = 1 << 6;
        const MultipleBlockSelect = 1 << 5;
        const WriteToCard = 1 << 4;
        const BlockCountEnable = 1 << 1;
        const DMAEnable = 1;
    }
}

const SD_CLASS_CODE: u8 = 0x8;
const SD_SUB_CLASS: u8 = 0x5;
const SD_NO_DMA_INTERFACE: u8 = 0x0;
const SD_DMA_INTERFACE: u8 = 0x1;
const SD_VENDOR_UNIQUE_INTERFACE: u8 = 0x2;

/// Finds the device that represents an SD card. It must support DMA, even
/// if the current driver does not support DMA. If the SD card was not found
/// returns Option::None, but if the card was cound, returns the SD card
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

/// Sets up an sd card
pub fn initalize_sd_card(
    sd_card: &DeviceInfo,
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut BootIntoFrameAllocator,
) -> Option<SDCardInfo> {
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
    // Wait for stuff to initalize
    let capablities = unsafe { core::ptr::read_volatile((bar_address + 0x40) as *const u64) };
    debug_println!("Capablities = 0x{capablities:X}");
    let version_address = (bar_address + 0xFE) as *const u16;

    let mut version_data = unsafe { core::ptr::read_volatile(version_address) };
    debug_println!("Version Data = 0x{version_data:X}");
    version_data &= 0xFF;

    // Need to reset card
    // Voltage check
    //
    let info = SDCardInfo {
        device_info: sd_card.clone(),
        cababilities: capablities,
        base_address_register: bar_address,
        version: version_data
            .try_into()
            .expect("Should have masked out upper byte"),
    };

    let reset_successfull = reset_sd_card(&info);
    match reset_successfull {
        Err(_) => return Option::None,
        Ok(_) => (),
    }

    debug_println!("Base clock: 0x{}", (capablities & 0xFFFF) >> 8);

    return Option::Some(info);
}

fn reset_sd_card(sd_card: &SDCardInfo) -> Result<(), SDCardError> {
    // DIsable all interupts
    let normal_intr_enable_addr = (sd_card.base_address_register + 0x38) as *mut u16;
    unsafe { core::ptr::write_volatile(normal_intr_enable_addr, 0) };

    // Use reset register
    let reset_addr = (sd_card.base_address_register + 0x2f) as *mut u8;
    unsafe { core::ptr::write_volatile(reset_addr, 1) };

    // Wait for reset to set in TODO: Remove spin
    let mut finished = false;
    for _ in 0..5_000_000 {
        let reset_taken = unsafe { core::ptr::read_volatile(reset_addr) };
        if reset_taken & 1 == 0 {
            finished = true;
            break;
        }
    }
    if !finished {
        return Result::Err(SDCardError::SDTimeout);
    }

    // Set timeouts to the max value
    let timeout_addr = (sd_card.base_address_register + 0x2e) as *mut u8;
    unsafe { core::ptr::write_volatile(timeout_addr, 0b1110) };

    // Re-enable interrupts

    let normal_intr_status_addr = (sd_card.base_address_register + 0x34) as *mut u16;
    unsafe { core::ptr::write_volatile(normal_intr_status_addr, 0xFF) };
    let error_intr_status_addr = (sd_card.base_address_register + 0x36) as *mut u16;
    unsafe { core::ptr::write_volatile(error_intr_status_addr, 0xFFF) };
    unsafe { core::ptr::write_volatile(normal_intr_enable_addr, 0xFF) };
    let error_intr_enable_addr = (sd_card.base_address_register + 0x40) as *mut u16;
    unsafe { core::ptr::write_volatile(error_intr_enable_addr, 0xFFF) };

    return Result::Ok(());
    // Re-enable interrupts
}

fn send_sd_command(
    sd_card: &SDCardInfo,
    command_idx: u8,
    respone_type: u8,
    index_check: bool,
    crc_check: bool,
    wait: bool,
) -> Result<(), SDCardError> {
    assert!(command_idx < 64);
    assert!(respone_type < 4);
    let present_state_register_addr = (sd_card.base_address_register + 0x24) as *const u32;

    let present_state = unsafe { core::ptr::read_volatile(present_state_register_addr) };
    if present_state & 0x3 != 0 || present_state & (1 << 27) != 0 {
        return Result::Err(SDCardError::CommandInhibited);
    }
    if present_state & (1 << 27) != 0 {
        return Result::Err(SDCardError::CommandStoppedDueToError);
    }

    let command_register_addr = (sd_card.base_address_register + 0xE) as *mut u16;
    let mut command: u16 = command_idx.into();
    command <<= 8;
    let index_check_int: u16 = index_check.into();
    command |= index_check_int << 4;
    let crc_check_int: u16 = crc_check.into();
    command |= crc_check_int << 3;
    let response_type_extended: u16 = respone_type.into();
    command |= response_type_extended;
    unsafe { core::ptr::write_volatile(command_register_addr, command) };
    if wait {
        let interrupt_status_register = (sd_card.base_address_register + 0x30) as *mut u16;
        loop {
            unsafe {
                let mut command_done = core::ptr::read_volatile(interrupt_status_register);
                if (command_done & 1) == 1 {
                    command_done &= 0xFFFE;
                    core::ptr::write_volatile(interrupt_status_register, command_done);
                    return Result::Ok(());
                }
            }
        }
    }
    return Result::Ok(());
}

/// Reads data from a sd card
pub fn read_sd_card(sd_card: &SDCardInfo, block: u32) -> Option<[u8; 512]> {
    let block_size_register_addr = (sd_card.base_address_register + 0x4) as *mut u16;
    unsafe { core::ptr::write_volatile(block_size_register_addr, 0x200) };
    let block_count_register_addr = (sd_card.base_address_register + 0x6) as *mut u16;
    unsafe { core::ptr::write_volatile(block_count_register_addr, 1) };

    let argument_register_addr = (sd_card.base_address_register + 0x8) as *mut u32;
    unsafe { core::ptr::write_volatile(argument_register_addr, block) };
    let transfer_mode_register_adder = (sd_card.base_address_register + 0xC) as *mut u16;
    unsafe {
        core::ptr::write_volatile(
            transfer_mode_register_adder,
            (TransferModeFlags::ResponseErrorChecKEnable).bits(),
        )
    };

    // Send command
    debug_println!("Before sending cmd 57");
    let success = send_sd_command(&sd_card, 57, 0b10, true, true, false);
    if success.is_err() {
        debug_println!("it failed");
        return Option::None;
    }

    return Option::None;
}
