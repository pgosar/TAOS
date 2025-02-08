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
#[allow(dead_code)]
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

/// No suppoort for R2 type responses
#[allow(dead_code)]
#[derive(Debug)]
enum SDCommandResponse {
    NoResponse,
    Response32Bits(u32),
}

#[allow(dead_code)]
enum SDResponseTypes {
    R1,
    R1b,
    R2,
    R3,
    R4,
    R5,
    R5b,
    R6,
    R7,
    NoResponse,
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
#[allow(dead_code)]
const SD_VENDOR_UNIQUE_INTERFACE: u8 = 0x2;
const MAX_ITERATIONS: usize = 100;
const SD_MAX_FREQUENCY_MHZ: u8 = 25;
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
/// THIS ASSUMES A VERSION 2 SDCARD as of writing
pub fn initalize_sd_card(
    sd_card: &DeviceInfo,
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut BootIntoFrameAllocator,
) -> Result<SDCardInfo, SDCardError> {
    // Assume sd_card is a device info for an SD Crd
    // Lets assume 1 slot, and it uses BAR 1

    // Disable Commands from being sent over the Memory Space
    let command = sd_card.command & !(COMMAND_MEMORY_SPACE);
    unsafe {
        write_pci_command(sd_card.bus, sd_card.device, 0, command);
    }

    // Determine the Base Address, and setup a mapping
    let base_address_register = unsafe { read_config(sd_card.bus, sd_card.device, 0, 0x10) };
    let bar_address: u64 = (base_address_register & 0xFFFFFF00).into();
    let bar_frame = PhysFrame::from_start_address(PhysAddr::new(bar_address)).unwrap();

    let page = Page::containing_address(VirtAddr::new(bar_address));
    paging::create_uncachable_mapping_given_frame(page, mapper, bar_frame, frame_allocator);

    // Re-enable memory space commands
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

    // Store Version of host
    let version_address = (bar_address + 0xFE) as *const u16;
    let mut version_data = unsafe { core::ptr::read_volatile(version_address) };
    debug_println!("Version Data = 0x{version_data:X}");
    version_data &= 0xFF;

    // Construct the stuct that we will use for further communication
    let info = SDCardInfo {
        device_info: sd_card.clone(),
        cababilities: capablities,
        base_address_register: bar_address,
        version: version_data
            .try_into()
            .expect("Should have masked out upper byte"),
    };

    reset_sd_card(&info)?;
    // TODO: Set clock appropately (see Clock control (0x2c))
    // Also look into Host control 2 Preset value enable
    // But with preset value, check that it exists and is non-zero
    // Look into Figure 2-29, for tuning

    // TODO: check that timeout orks

    // TODO: Not just in this function, check erorr interrupt status register
    // TODO: Not in this function, deal with repeated code in read / write


    return Result::Ok(info);
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
        debug_println!("Reset Failed");
        return Result::Err(SDCardError::SDTimeout);
    }

    // Turn power on

    // Read capablities()
    let power_controll_addr = (sd_card.base_address_register + 0x29) as *mut u8;
    let mut power_control: u8 = ((sd_card.cababilities >> 24 ) & 0b111).try_into().expect("Trimmed out bits");
    power_control <<= 1;
    power_control |= 1;
    unsafe { core::ptr::write_volatile(power_controll_addr, power_control) };
    

    // Determine Set Clock Frequency
    let base_clock: u8 = ((sd_card.cababilities >> 8 ) & 0xFF).try_into().expect("Trimmed out bits");
    // We need to ensure this is less than SD_MAX_FREQUENCY_MHZ
    let mut divisor = 1;
    // TODO: fix error
    let mut divisor_set = false;
    for _ in 0..8 {
        debug_println!("Base clock is {base_clock}");
        let frequency = base_clock / divisor;
        debug_println!("Divisor is {divisor}");
        if frequency < SD_MAX_FREQUENCY_MHZ {
            // Set Divisor
            divisor_set = true;
            break;
        }
        divisor <<= 1;
    }
    if !divisor_set {
        // TODO: return better error
        debug_println!("Reset did not take");
        return Result::Err(SDCardError::SDTimeout)
    }

    let clock_ctrl_reg_addr =  (sd_card.base_address_register + 0x2c) as *mut u16;
    // Disable clock
    unsafe { core::ptr::write_volatile(clock_ctrl_reg_addr, 0) };
    let mut clock_ctrl_reg:u16 = divisor.into();
    clock_ctrl_reg <<= 8;
    // clock_ctrl_reg |= 1 << 5;

    // Set clock to something
    unsafe { core::ptr::write_volatile(clock_ctrl_reg_addr, clock_ctrl_reg) };
    // Enable internal clock
    clock_ctrl_reg |= 1;
    unsafe { core::ptr::write_volatile(clock_ctrl_reg_addr, clock_ctrl_reg) };
    // Wait for stability
    let mut finished = false;
    for _ in 0..MAX_ITERATIONS {
        let clock_taken = unsafe { core::ptr::read_volatile(clock_ctrl_reg_addr) };
        if clock_taken & 0b10 != 0 {
            finished = true;
            break;
        }
        core::hint::spin_loop();
    }
    if !finished {
        debug_println!("Clock set Failed");
        return Result::Err(SDCardError::SDTimeout);
    }

    // Send clock to SD 
    clock_ctrl_reg |= 1 << 2;
    unsafe { core::ptr::write_volatile(clock_ctrl_reg_addr, clock_ctrl_reg) };


    // Set timeouts to the max value
    let timeout_addr = (sd_card.base_address_register + 0x2e) as *mut u8;
    unsafe { core::ptr::write_volatile(timeout_addr, 0b1110) };
    let _ = check_valid_sd_card(sd_card)?;
    // Re-enable interrupts, clearing out anything that was present (RW1C)

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
}

fn check_valid_sd_card(sd_card: &SDCardInfo) -> Result<(), SDCardError> {
    check_no_errors(sd_card)?;
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

fn check_no_errors(sd_card: &SDCardInfo) -> Result<(), SDCardError> {
    let error_state_intr_addr = (sd_card.base_address_register + 0x32) as *const u16;
    let error_state = unsafe {core::ptr::read_volatile(error_state_intr_addr)};
    if error_state != 0 {
        debug_println!("Error detected 0x{error_state:x}");
        // TODO: Give better error
        return Result::Err(SDCardError::CommandStoppedDueToError);
    }

    return Result::Ok(());
}

/// Bugs: Does not currently work with cmd 12 or 23 (gets response in wrong place)
/// Also Response type r7 does not seem to be documented
fn send_sd_command(
    sd_card: &SDCardInfo,
    command_idx: u8,
    respone_type: SDResponseTypes,
    flags: CommandFlags,
    wait: bool,
) -> Result<SDCommandResponse, SDCardError> {
    assert!(command_idx < 64);
    let _ = check_valid_sd_card(sd_card).unwrap();

    let command_register_addr = (sd_card.base_address_register + 0xE) as *mut u16;
    let mut command: u16 = command_idx.into();
    command <<= 8;
    let mut myflags = CommandFlags::from_bits_retain(flags.bits());
    match respone_type {
        SDResponseTypes::R2 => {
            command |= 0b01;
            myflags |= CommandFlags::CommandCRCCheckEnable
        }
        SDResponseTypes::R3 | SDResponseTypes::R4 => {
            command |= 0b10;
        }
        SDResponseTypes::R1 | SDResponseTypes::R5 => {
            command |= 0b10;
            myflags |= CommandFlags::CommandIndexCheckEnable | CommandFlags::CommandCRCCheckEnable;
        }
        SDResponseTypes::R1b | SDResponseTypes::R5b => {
            command |= 0b11;
            myflags |= CommandFlags::CommandIndexCheckEnable | CommandFlags::CommandCRCCheckEnable;
        }
        _ => (),
    }
    command |= myflags.bits();
    // let response_type_extended: u16 = respone_type.into();
    // command |= response_type_extended;
    let _ = check_valid_sd_card(sd_card).unwrap();
    unsafe { core::ptr::write_volatile(command_register_addr, command) };
    check_no_errors(sd_card).unwrap();
    if wait {
        let interrupt_status_register = (sd_card.base_address_register + 0x30) as *mut u16;
        for _ in 0..MAX_ITERATIONS {
            unsafe {
                let mut command_done = core::ptr::read_volatile(interrupt_status_register);
                if (command_done & 1) == 1 {
                    command_done &= 0xFFFE;
                    core::ptr::write_volatile(interrupt_status_register, command_done);
                    return Result::Ok(SDCommandResponse::NoResponse);
                }
            }
        }
        check_no_errors(sd_card).unwrap();
        return Result::Err(SDCardError::SDTimeout);
    }
    let _ = check_valid_sd_card(sd_card)?;
    check_no_errors(sd_card).unwrap();

    let response_register = (sd_card.base_address_register + 0x10) as *const u32;
    match respone_type {
        SDResponseTypes::R1b
        | SDResponseTypes::R1
        | SDResponseTypes::R3
        | SDResponseTypes::R4
        | SDResponseTypes::R5b
        | SDResponseTypes::R5
        | SDResponseTypes::R6 => {
            let response = unsafe { core::ptr::read_volatile(response_register) };
            return Result::Ok(SDCommandResponse::Response32Bits(response));
        },
        _ => return Result::Ok(SDCommandResponse::NoResponse),
    }
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
    unsafe { core::ptr::write_volatile(transfer_mode_register_adder, 0) };

    // Send command
    debug_println!("Before sending cmd 17");
    send_sd_command(
        &sd_card,
        17,
        SDResponseTypes::R1,
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
            (TransferModeFlags::WriteToCard).bits(),
        )
    };

    // Send command
    debug_println!("Before sending cmd 24");
    send_sd_command(
        &sd_card,
        24,
        SDResponseTypes::R1,
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
