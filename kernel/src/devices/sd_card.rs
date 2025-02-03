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

#[repr(C)]
struct SDHostControllerRegisterSet {
    sdma_address: u32,
    block_size: u16,
    block_count: u16,
    argument: u32,
    transfer_mode: u16,
    command: u16,
    response_0: u16,
    response_1: u16,
    response_2: u16,
    response_3: u16,
    response_4: u16,
    response_5: u16,
    response_6: u16,
    response_7: u16,
    buffer_data_port_0: u16,
    buffer_data_port_1: u16,
    present_state_0: u16,
    present_state_1: u16,
    host_control_1: u8,
    power_control: u8,
    block_gap_control: u8,
    wakeup_control: u8,
    clock_control: u16,
}

#[derive(Debug)]
struct SDHostControllerData {
    data: [u16; 128],
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
    debug_println!("We have {} slots", (slot_info_register & 0x70 >> 4) + 1);
    debug_println!("Full bar is  0x{slot_info_register:X}");

    let command_stauts = unsafe { read_config(sd_card.bus, sd_card.device, 0, 0x4) };
    debug_println!("status = 0x{}", command_stauts & 0x0000FFFF);
    debug_println!("Command = 0x{}", command_stauts >> 16);

    let command = sd_card.command & !(COMMAND_MEMORY_SPACE);
    unsafe {
        write_pci_command(sd_card.bus, sd_card.device, 0, command);
    }
    let base_address_register = unsafe { read_config(sd_card.bus, sd_card.device, 0, 0x10) };
    unsafe {
        write_pci_data(sd_card.bus, sd_card.device, 0, 0x10, 0xFFFFFFFF);
    }
    let bar_size = unsafe { read_config(sd_card.bus, sd_card.device, 0, 0x10) };
    debug_println!("Bar space indicator is 0x{:X}", base_address_register & 0x1);
    debug_println!("Bar Location is 0x{:X}", base_address_register & 0x3 >> 1);
    debug_println!("Bar size is 0x{:X}", bar_size);
    debug_println!("Bar is 0x{:X}", base_address_register);
    debug_println!(
        "Bar IO base addr is 0x{:X}",
        base_address_register & 0xFFFFFF00
    );
    let bar_address: u64 = (base_address_register & 0xFFFFFF00).into();
    let bar_frame = PhysFrame::from_start_address(PhysAddr::new(bar_address)).unwrap();

    // bar_address += 0xFFFF800000000000;

    // deb
    // unsafe {
    //     write_pci_command(
    //         sd_card.bus,
    //         sd_card.device,
    //         0,
    //         sd_card.command | COMMAND_MEMORY_SPACE | COMMAND_IO_SPACE | COMMAND_BUS_MASTER,
    //     );
    // }

    let page = Page::containing_address(VirtAddr::new(bar_address));
    debug_println!("Page = {page:?}");
    // let frame = PhysAddr::new(bar_address);

    paging::create_uncachable_mapping_given_frame(page, mapper, bar_frame, frame_allocator);
    // paging::create_mapping(page, mapper, frame_allocator);
    // TODO remove unwrap / ensure that things work well
    let phys = mapper.translate_addr(VirtAddr::new(bar_address)).unwrap();
    unsafe {
        let phys_int = phys.as_u64();
        let phys_lower: u32 = phys_int.try_into().unwrap();
        write_pci_data(sd_card.bus, sd_card.device, 0, 0x10, phys_lower);
        write_pci_command(
            sd_card.bus,
            sd_card.device,
            0,
            sd_card.command | COMMAND_MEMORY_SPACE,
        );
    }

    debug_println!("{:?} -> {:?}", VirtAddr::new(bar_address), phys);
    // Weve mapped stuff to physcial memory, mAYBE

    let block_count_ptr = (bar_address + 0x24) as *const u32;
    // unsafe { core::ptr::read_volatile(block_count_ptr) }
    loop {
        let block_count = unsafe { core::ptr::read_volatile(block_count_ptr) };
        if block_count != 0 {
            debug_println!("block_count is {}", block_count);
            break;
        }
        core::hint::spin_loop();
        ctr += 1
    }


    let capability = (bar_address + 0x40) as *const u64;
    loop {
        if (unsafe { *capability } != 0) {
            break;
        }
        core::hint::spin_loop();
    }
    unsafe {
        debug_println!("cap is 0x{:X}", *capability);
    }
    let host_ctrl_2 = (bar_address + 0x3E) as *const u64;

    unsafe {
        debug_println!("Host ctrl 2 is 0x{:X}", *host_ctrl_2);
    }
}
