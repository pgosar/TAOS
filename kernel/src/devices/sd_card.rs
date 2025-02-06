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

#[derive(Debug)]
pub enum SDCardError {
    CommandInhibited,
    CommandStoppedDueToError,
    SDTimeout,
}

bitflags! {
    struct TransferModeFlags: u16 {
        const ResponseInterruptDisable = 1 << 8;
        const ResponseErrorChecKEnable = 1 << 7;
        const ResponseTypeSDIO = 1 << 6;
        const MultipleBlockSelect = 1 << 5;
        const WriteToCard = 1 << 4;
        const BlockCountEnable = 1 << 1;
        const DMAEnable = 1;
    }
}

bitflags! {
    struct Capablities: u64 {
        const VDD2SSupported = 1 << 60;
        const ADMA3Supported = 1 << 59;
        const SDR50TuningRequired = 1 << 45;
        const DriverTypeDSupprted = 1 << 38;
        const DriverTypeCSupprted = 1 << 37;
        const DriverTypeASupprted = 1 << 36;
        const UHS2Support = 1 << 35;
        const DDR50Support = 1 << 34;
        const SDR104Suppport = 1 << 33;
        const SDR50Support = 1 << 32;
        const AsynchronousInterruptSupport = 1 << 29;
        const SystemAddress64SupportV3Mode = 1 << 28;
        const SystemAddress64SupportV4Mode = 1 << 27;
        const Voltage1_8Support = 1 << 26;
        const Voltage3_0Support = 1 << 25;
        const Voltage3_3Support = 1 << 24;
        const SuspendSupport = 1 << 23;
        const SDMASupport = 1 << 22;
        // Allows SD clock frequency to go from 25MHZ to 50MHz
        const HighSpeedSupport = 1 << 21;
        const ADMA2Support = 1 << 19;
        const Embedded8bitSupport = 1 << 18;
        // If not set uses Kkz to set timeout
        const TimeoutClockMhz = 1 << 7;
        const _ = !0;
    }
}

bitflags! {
    struct PresentState: u32 {
        const UHS2IFDetection = 1 << 31;
        const LaneSynchronization = 1 << 30;
        const DormantState = 1 << 29;
        const SubCommandStatus = 1 << 28;
        const CommandNotIssuedError = 1 << 27;
        const HostRegulatorVoltageStable = 1 << 25;
        const CMDLineSignalLevel = 1 << 24;
        const WriteEnabled  = 1 << 19;
        const CardPresent = 1 << 18;
        const CardStateStable = 1 << 17;
        const CardInserted = 1 << 16;
        const BufferReadEnable = 1 << 11;
        const BufferWriteEnable = 1 << 10;
        const ReadTransferActive = 1 << 9;
        const WriteTransferActive = 1 << 8;
        const RetuningRequest = 1 << 3;
        const DATLineActive = 1 << 2;
        const CommandInhibitData = 1 << 1;
        const CommandInhibitCmd = 1;
        const _ = !0;
    }
}

bitflags! {
    struct CommandFlags: u16 {
        const DataPresentSelect = 1 << 5;
        const CommandIndexCheckEnable = 1 << 4;
        const CommandCRCCheckEnable = 1 << 3;
        const SubCommandFlag = 1 << 2;
        const _ = !0;
    }
}

const SD_CLASS_CODE: u8 = 0x8;
const SD_SUB_CLASS: u8 = 0x5;
const SD_NO_DMA_INTERFACE: u8 = 0x0;
const SD_DMA_INTERFACE: u8 = 0x1;
const SD_VENDOR_UNIQUE_INTERFACE: u8 = 0x2;
const MAX_ITERATIONS: usize = 5_000_000;
/// Finds the FIRST device that represents an SD card, or returns None if
/// this was not found. Most functions take in SDCard Info struct, which
/// can be recieved by using initalize_sd_card with the SD card that
/// was found using this function.
pub fn find_sd_card(devices: &AllDeviceInfo) -> Option<&DeviceInfo> {
    for possible_device in &devices.device_info {
        match possible_device {
            None => (),
            Some(device) => {
                if device.class_code == SD_CLASS_CODE
                    && device.subclass == SD_SUB_CLASS
                    && (device.programming_interface == SD_DMA_INTERFACE
                        || device.programming_interface == SD_NO_DMA_INTERFACE)
                {
                    return Option::Some(device);
                }
            }
        }
    }
    return Option::None;
}

/// Sets up an sd card, returning an SDCardInfo that can be used for further
/// accesses to the sd card
pub fn initalize_sd_card(
    sd_card: &DeviceInfo,
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut BootIntoFrameAllocator,
) -> Option<SDCardInfo> {
    // Assume sd_card is a device info
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

    // Store capabilities in capabilties register
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
    // TODO: Set clock appropately (see Clock control (0x2c))
    // Also look into Host control 2 Preset value enable
    // But with preset value, check that it exists and is non-zero
    // Look into Figure 2-29, for tuning


    // TODO: check that timeout orks

    // TODO: Not just in this function, check erorr interrupt status register
    // TODO: Not in this function, deal with repeated code in read / write

    // TODO: Make this a result

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
    for _ in 0..MAX_ITERATIONS {
        let reset_taken = unsafe { core::ptr::read_volatile(reset_addr) };
        if reset_taken & 1 == 0 {
            finished = true;
            break;
        }
        core::hint::spin_loop();
    }
    if !finished {
        return Result::Err(SDCardError::SDTimeout);
    }

    // Set timeouts to the max value
    let timeout_addr = (sd_card.base_address_register + 0x2e) as *mut u8;
    unsafe { core::ptr::write_volatile(timeout_addr, 0b1110) };
    let _ = check_valid_sd_card(sd_card)?;
    // Re-enable interrupts

    let normal_intr_status_addr = (sd_card.base_address_register + 0x34) as *mut u16;
    unsafe { core::ptr::write_volatile(normal_intr_status_addr, 0xFF) };
    let _ = check_valid_sd_card(sd_card)?;
    let error_intr_status_addr = (sd_card.base_address_register + 0x36) as *mut u16;
    unsafe { core::ptr::write_volatile(error_intr_status_addr, 0xFFF) };
    let _ = check_valid_sd_card(sd_card)?;
    unsafe { core::ptr::write_volatile(normal_intr_enable_addr, 0xFF) };
    let _ = check_valid_sd_card(sd_card)?;
    let error_intr_enable_addr = (sd_card.base_address_register + 0x40) as *mut u16;
    unsafe { core::ptr::write_volatile(error_intr_enable_addr, 0xFFF) };
    let _ = check_valid_sd_card(sd_card)?;

    return Result::Ok(());
    // Re-enable interrupts
}

fn check_valid_sd_card(sd_card: &SDCardInfo) -> Result<(), SDCardError> {
    let present_state_register_addr = (sd_card.base_address_register + 0x24) as *const u32;
    let present_state = unsafe { core::ptr::read_volatile(present_state_register_addr) };
    let inhibited_state = PresentState::CommandInhibitCmd | PresentState::CommandInhibitData;
    if inhibited_state.contains(PresentState::from_bits_retain(present_state)) {
        return Result::Err(SDCardError::CommandInhibited);
    }
    if PresentState::CommandNotIssuedError.contains(PresentState::from_bits_retain(present_state)) {
        return Result::Err(SDCardError::CommandStoppedDueToError);
    }
    return Result::Ok(());
}

fn send_sd_command(
    sd_card: &SDCardInfo,
    command_idx: u8,
    respone_type: u8,
    flags: CommandFlags,
    wait: bool,
) -> Result<(), SDCardError> {
    assert!(command_idx < 64);
    assert!(respone_type < 4);

    let _ = check_valid_sd_card(sd_card)?;

    let command_register_addr = (sd_card.base_address_register + 0xE) as *mut u16;
    let mut command: u16 = command_idx.into();
    command <<= 8;
    command |= flags.bits();
    let response_type_extended: u16 = respone_type.into();
    command |= response_type_extended;
    let _ = check_valid_sd_card(sd_card)?;
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
    let _ = check_valid_sd_card(sd_card)?;
    return Result::Ok(());
}

/// Reads data from a sd card
pub fn read_sd_card(sd_card: &SDCardInfo, block: u32) -> Result<[u32; 128], SDCardError> {
    let block_size_register_addr = (sd_card.base_address_register + 0x4) as *mut u16;
    unsafe { core::ptr::write_volatile(block_size_register_addr, 0x200) };
    let block_count_register_addr = (sd_card.base_address_register + 0x6) as *mut u16;
    unsafe { core::ptr::write_volatile(block_count_register_addr, 1) };
    check_valid_sd_card(sd_card)?;
    let argument_register_addr = (sd_card.base_address_register + 0x8) as *mut u32;
    unsafe { core::ptr::write_volatile(argument_register_addr, block) };
    let transfer_mode_register_adder = (sd_card.base_address_register + 0xC) as *mut u16;
    unsafe {
        core::ptr::write_volatile(
            transfer_mode_register_adder,
            (TransferModeFlags::ResponseErrorChecKEnable
                | TransferModeFlags::ResponseInterruptDisable)
                .bits(),
        )
    };

    // Send command
    debug_println!("Before sending cmd 57");
    send_sd_command(
        &sd_card,
        57,
        0b10,
        CommandFlags::DataPresentSelect
            | CommandFlags::CommandCRCCheckEnable
            | CommandFlags::CommandIndexCheckEnable,
        false,
    )?;

    let present_state_register_addr = (sd_card.base_address_register + 0x24) as *const u32;
    let mut finished = false;
    for _ in 0..MAX_ITERATIONS {
        let present_state = unsafe {
            PresentState::from_bits_retain(core::ptr::read_volatile(present_state_register_addr))
        };
        if PresentState::BufferReadEnable.contains(present_state) {
            finished = true;
            break;
        }
        core::hint::spin_loop();
    }
    if !finished {
        return Result::Err(SDCardError::SDTimeout);
    }

    let mut data = [0; 128];
    let buffer_data_port_reg_addr = (sd_card.base_address_register + 0x20) as *const u32;
    for i in 0..128 {
        data[i] = unsafe { core::ptr::read_volatile(buffer_data_port_reg_addr) };
    }

    return Result::Ok(data);
}

pub fn write_sd_card(
    sd_card: &SDCardInfo,
    block: u32,
    data: [u32; 128],
) -> Result<(), SDCardError> {
    let block_size_register_addr = (sd_card.base_address_register + 0x4) as *mut u16;
    unsafe { core::ptr::write_volatile(block_size_register_addr, 0x200) };
    let block_count_register_addr = (sd_card.base_address_register + 0x6) as *mut u16;
    unsafe { core::ptr::write_volatile(block_count_register_addr, 1) };

    check_valid_sd_card(sd_card)?;
    let argument_register_addr = (sd_card.base_address_register + 0x8) as *mut u32;
    unsafe { core::ptr::write_volatile(argument_register_addr, block) };
    let transfer_mode_register_adder = (sd_card.base_address_register + 0xC) as *mut u16;
    unsafe {
        core::ptr::write_volatile(
            transfer_mode_register_adder,
            (TransferModeFlags::ResponseErrorChecKEnable
                | TransferModeFlags::WriteToCard
                | TransferModeFlags::ResponseInterruptDisable)
                .bits(),
        )
    };

    // Send command
    debug_println!("Before sending cmd 57");
    send_sd_command(
        &sd_card,
        57,
        0b10,
        CommandFlags::DataPresentSelect
            | CommandFlags::CommandCRCCheckEnable
            | CommandFlags::CommandIndexCheckEnable,
        false,
    )?;

    let present_state_register_addr = (sd_card.base_address_register + 0x24) as *const u32;
    let mut finished = false;
    for _ in 0..MAX_ITERATIONS {
        let present_state = unsafe {
            PresentState::from_bits_retain(core::ptr::read_volatile(present_state_register_addr))
        };
        if PresentState::BufferReadEnable.contains(present_state) {
            finished = true;
            break;
        }
        core::hint::spin_loop();
    }
    if !finished {
        return Result::Err(SDCardError::SDTimeout);
    }

    let buffer_data_port_reg_addr = (sd_card.base_address_register + 0x20) as *mut u32;
    for i in 0..128 {
        unsafe {
            core::ptr::write_volatile(buffer_data_port_reg_addr, data[i]);
        }
    }

    return Result::Ok(());
}
