use x86_64::{
    structures::paging::{
        mapper::{MappedFrame, TranslateResult},
        Mapper, OffsetPageTable, Page, PageTableFlags, PhysFrame, Size1GiB, Size2MiB, Size4KiB,
        Translate,
    },
    PhysAddr, VirtAddr,
};

use crate::{
    debug_println,
    devices::pci::{write_pci_command, COMMAND_BUS_MASTER, COMMAND_MEMORY_SPACE},
    filesys::{BlockDevice, FsError},
    memory::paging,
};
use bitflags::{bitflags, Flag};

use super::pci::{read_config, AllDeviceInfo, DeviceInfo};

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct SDCardInfoInternal {
    device_info: DeviceInfo,
    capabilities: u64,
    base_address_register: u64,
    version: u8,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct SDCardInfo {
    internal_info: SDCardInfoInternal,
    block_size: usize,
    total_blocks: u64,
    reletave_card_address: u32,
}

#[derive(Debug)]
pub enum SDCardError {
    CommandInhibited,
    CommandStoppedDueToError,
    SDTimeout,
    FrequencyUnableToBeSet,
    VoltageUnableToBeSet,
    GenericSDError,
}

/// No support for R2 type responses
#[allow(dead_code)]
#[derive(Debug)]
enum SDCommandResponse {
    NoResponse,
    Response32Bits(u32),
    Response128Bits(u128),
}

#[allow(dead_code)]
enum SDResponseTypes {
    R0,
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
        const ReadToCard = 1 << 4;
        const BlockCountEnable = 1 << 1;
        const DMAEnable = 1;
    }
}

bitflags! {
    struct Capabilities: u64 {
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
const MAX_ITERATIONS: usize = 1_000;
const SD_BLOCK_SIZE: u32 = 512;

impl BlockDevice for SDCardInfo {
    fn read_block(&self, block_num: u64, buf: &mut [u8]) -> Result<(), FsError> {
        if block_num > self.total_blocks {
            return Result::Err(FsError::IOError);
        }
        let data = read_sd_card(
            &self,
            block_num
                .try_into()
                .expect("Maxumum block number should not be greater than 32 bits"),
        )
        .map_err(|_| FsError::IOError)?;
        buf.copy_from_slice(&data);

        return Result::Ok(());
    }

    fn write_block(&mut self, block_num: u64, buf: &[u8]) -> Result<(), FsError> {
        if block_num > self.total_blocks {
            return Result::Err(FsError::IOError);
        }
        let mut data: [u8; 512] = [0; 512];
        data.copy_from_slice(buf);
        write_sd_card(
            &self,
            block_num
                .try_into()
                .expect("Maximum block number should not be greater than 32 bits"),
            data,
        )
        .map_err(|_| FsError::IOError)?;
        return Result::Ok(());
    }
    fn block_size(&self) -> usize {
        return SD_BLOCK_SIZE.try_into().expect("To be on 64 bit system");
    }

    fn total_blocks(&self) -> u64 {
        return 512;
        return self.total_blocks;
    }
}

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
/// THIS FUNCTION IS NOT THREAD SAFE (Ie dont try to initalize the same thread twice)
pub unsafe fn initalize_sd_card(
    sd_card: &DeviceInfo,
    mapper: &mut OffsetPageTable,
    offset: u64,
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
    let bar_frame: PhysFrame<Size4KiB> =
        PhysFrame::from_start_address(PhysAddr::new(bar_address)).unwrap();

    let offset_bar = bar_address + offset;
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
                        .map_err(|_| SDCardError::GenericSDError)?
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
                        .map_err(|_| SDCardError::GenericSDError)?
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
                        .map_err(|_| SDCardError::GenericSDError)?
                        .flush();
                }
            }
        },
        TranslateResult::InvalidFrameAddress(_) => {
            panic!("Invalid physical address in SD BAR")
        }
        TranslateResult::NotMapped => {
            let page: Page<Size4KiB> = Page::containing_address(VirtAddr::new(offset_bar));
            let bar_frame: PhysFrame<Size4KiB> =
                PhysFrame::containing_address(PhysAddr::new(bar_address));
            paging::create_uncachable_mapping(page, bar_frame, mapper);
        }
    }
    // Re-enable memory space commands
    unsafe {
        write_pci_command(
            sd_card.bus,
            sd_card.device,
            0,
            sd_card.command | COMMAND_MEMORY_SPACE | COMMAND_BUS_MASTER,
        );
    }
    // Store capabilities in capabilties register
    let capablities = unsafe { core::ptr::read_volatile((offset_bar + 0x40) as *const u64) };

    // Store Version of host
    let version_address = (offset_bar + 0xFE) as *const u16;
    let mut version_data = unsafe { core::ptr::read_volatile(version_address) };
    version_data &= 0xFF;

    // Construct the stuct that we will use for further communication
    let info = SDCardInfoInternal {
        device_info: sd_card.clone(),
        capabilities: capablities,
        base_address_register: offset_bar,
        version: version_data
            .try_into()
            .expect("Should have masked out upper byte"),
    };

    let new_info = reset_sd_card(&info)?;
    return Result::Ok(new_info);
}

/// Sends a software reset to the sd card using the reset register
fn software_reset_sd_card(sd_card: &SDCardInfoInternal) -> Result<(), SDCardError> {
    debug_println!("Resetting");
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
    return Result::Ok(());
}

fn enable_sd_card_interrupts(sd_card: &SDCardInfoInternal) -> Result<(), SDCardError> {
    let normal_intr_status_addr = (sd_card.base_address_register + 0x34) as *mut u16;
    unsafe { core::ptr::write_volatile(normal_intr_status_addr, 0x1FF) };
    sending_command_valid(sd_card)?;
    let error_intr_status_addr = (sd_card.base_address_register + 0x36) as *mut u16;
    unsafe { core::ptr::write_volatile(error_intr_status_addr, 0x0FB) };
    sending_command_valid(sd_card)?;
    let normal_intr_enable_addr = (sd_card.base_address_register + 0x38) as *mut u16;
    unsafe { core::ptr::write_volatile(normal_intr_enable_addr, 0x1FF) };
    sending_command_valid(sd_card)?;
    let error_intr_enable_addr = (sd_card.base_address_register + 0x3A) as *mut u16;
    unsafe { core::ptr::write_volatile(error_intr_enable_addr, 0x0FB) };
    sending_command_valid(sd_card)?;
    return Result::Ok(());
}

fn power_on_sd_card(sd_card: &SDCardInfoInternal) -> Result<(), SDCardError> {
    let power_control_addr = (sd_card.base_address_register + 0x29) as *mut u8;
    // Turn off power so we can change the voltage
    unsafe { core::ptr::write_volatile(power_control_addr, 0) };
    // Use maximum voltage that capabiltieis shows
    let mut power_control;
    if Capabilities::Voltage3_3Support
        .intersects(Capabilities::from_bits_retain(sd_card.capabilities))
    {
        power_control = 0b111 << 1;
    } else if Capabilities::Voltage1_8Support
        .intersects(Capabilities::from_bits_retain(sd_card.capabilities))
    {
        power_control = 0b101 << 1;
    } else if Capabilities::Voltage3_0Support
        .intersects(Capabilities::from_bits_retain(sd_card.capabilities))
    {
        power_control = 0b110 << 1;
    } else {
        return Result::Err(SDCardError::VoltageUnableToBeSet);
    }
    unsafe { core::ptr::write_volatile(power_control_addr, power_control) };
    power_control |= 1;
    unsafe { core::ptr::write_volatile(power_control_addr, power_control) };
    return Result::Ok(());
}

fn set_sd_clock(sd_card: &SDCardInfoInternal) -> Result<(), SDCardError> {
    let clock_ctrl_reg_addr = (sd_card.base_address_register + 0x2c) as *mut u16;
    // Disable sd_clock so we can change it
    let mut clock_ctrl_reg = unsafe { core::ptr::read_volatile(clock_ctrl_reg_addr) };
    clock_ctrl_reg &= 0xFFFE;
    unsafe { core::ptr::write_volatile(clock_ctrl_reg_addr, clock_ctrl_reg) };

    // TODO check if actural hardware supports PLL enable (if so unset it too)
    clock_ctrl_reg &= 0x00FF;
    clock_ctrl_reg |= 0x80 << 8;
    // clock_ctrl_reg |= divisor_extended << 8;
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
    // Now we can enable the SD clock
    clock_ctrl_reg |= 0b11 << 1;
    unsafe { core::ptr::write_volatile(clock_ctrl_reg_addr, clock_ctrl_reg) };

    return Result::Ok(());
}

fn enable_timeouts(sd_card: &SDCardInfoInternal) -> Result<(), SDCardError> {
    let timeout_addr = (sd_card.base_address_register + 0x2e) as *mut u8;
    unsafe { core::ptr::write_volatile(timeout_addr, 0b1110) };
    sending_command_valid(sd_card)?;
    return Result::Ok(());
}

fn send_sd_reset_commands(sd_card: &SDCardInfoInternal) -> Result<SDCardInfo, SDCardError> {
    // Send cmd 0
    let mut result = Result::Err(SDCardError::GenericSDError);
    let argument_register_addr = (sd_card.base_address_register + 0x8) as *mut u32;
    unsafe { core::ptr::write_volatile(argument_register_addr, 0) };
    send_sd_command(sd_card, 0, SDResponseTypes::R0, CommandFlags::empty())?;
    // CMD 8
    let argument_register_addr = (sd_card.base_address_register + 0x8) as *mut u32;
    // TODO fix: Hard code voltage in 2.7-3.8v
    // check pattern = 0xaa
    unsafe { core::ptr::write_volatile(argument_register_addr, 0x1aa) };
    send_sd_command(sd_card, 8, SDResponseTypes::R7, CommandFlags::empty())?;
    // CMD 55
    unsafe { core::ptr::write_volatile(argument_register_addr, 0) };
    send_sd_command(sd_card, 55, SDResponseTypes::R1, CommandFlags::empty())?;

    // ACMD 41 (Inquiry)
    let acmd_response_enum =
        send_sd_command(sd_card, 41, SDResponseTypes::R3, CommandFlags::empty())?;
    if let SDCommandResponse::Response32Bits(acmd_response) = acmd_response_enum {
        unsafe { core::ptr::write_volatile(argument_register_addr, 0) };
        send_sd_command(sd_card, 0, SDResponseTypes::R0, CommandFlags::empty())?;
        // CMD 8
        let argument_register_addr = (sd_card.base_address_register + 0x8) as *mut u32;
        // TODO fix: Hard code voltage in 2.7-3.8v
        // check pattern = 0xaa
        unsafe { core::ptr::write_volatile(argument_register_addr, 0x1aa) };
        send_sd_command(sd_card, 8, SDResponseTypes::R7, CommandFlags::empty())?;
        // CMD 55
        unsafe { core::ptr::write_volatile(argument_register_addr, 0) };
        send_sd_command(sd_card, 55, SDResponseTypes::R1, CommandFlags::empty())?;

        // ACMD 41 (Inquiry)
        let mut new_acmd_response = acmd_response & 0xFF;
        // Enable sdhc
        new_acmd_response |= 1 << 22;
        unsafe { core::ptr::write_volatile(argument_register_addr, new_acmd_response) };
        send_sd_command(sd_card, 41, SDResponseTypes::R3, CommandFlags::empty())?;
    } else {
        panic!("ACMD should return a 32 bit response")
    }

    // cmd 2

    unsafe { core::ptr::write_volatile(argument_register_addr, 0) };
    send_sd_command(sd_card, 2, SDResponseTypes::R2, CommandFlags::empty())?;
    // cmd 3
    unsafe { core::ptr::write_volatile(argument_register_addr, 0) };
    let rca_enum = send_sd_command(sd_card, 3, SDResponseTypes::R6, CommandFlags::empty())?;
    if let SDCommandResponse::Response32Bits(rca) = rca_enum {
        unsafe { core::ptr::write_volatile(argument_register_addr, rca) };
        let csd_enum = send_sd_command(sd_card, 9, SDResponseTypes::R2, CommandFlags::empty())?;
        if let SDCommandResponse::Response128Bits(csd) = csd_enum {
            let actual_info = get_full_sd_card_info(sd_card, rca, csd)?;
            result = Result::Ok(actual_info);
        }

        unsafe { core::ptr::write_volatile(argument_register_addr, rca) };
        send_sd_command(sd_card, 13, SDResponseTypes::R1, CommandFlags::empty())?;

        // Sebd cnd 7 to set transfer state
        unsafe { core::ptr::write_volatile(argument_register_addr, rca) };
        send_sd_command(sd_card, 7, SDResponseTypes::R1b, CommandFlags::empty())?;
    } else {
        panic!("CMD 3 should return a 32 bit response");
    }

    // cmd 9
    return result;
}

fn get_full_sd_card_info(
    sd_card: &SDCardInfoInternal,
    rca: u32,
    csd: u128,
) -> Result<SDCardInfo, SDCardError> {
    // Currently only supports CSD Version 1.0
    let csd_structre: u32 = (csd >> 126)
        .try_into()
        .expect("Higher bits to be masked out");
    assert!(
        csd_structre == 0,
        "No support for SDHC or SDXC cards as of this moment"
    );
    let c_size: u32 = (csd >> 62 & 0xFFF)
        .try_into()
        .expect("Higher bits should be masked out");
    // let max_block_len: u32 = (csd >> 22 & 0xF)
    //     .try_into()
    //     .expect("Higher bits to be masked out");
    let c_size_mult: u32 = (csd >> 47 & 0b111)
        .try_into()
        .expect("Higher bits to be masked out");

    let mult = 1 << (c_size_mult + 2);
    let block_number = (c_size + 1) * mult;
    // let block_length = 1 << max_block_len;
    let block_length = SD_BLOCK_SIZE;
    let conversion_to_512_byte_blocks = block_length / SD_BLOCK_SIZE;
    let actual_block_number = block_number * conversion_to_512_byte_blocks;
    // TODO deternube wgy block length is wrong
    debug_println!("mult = {mult}, block_no = {block_number}, conversion = {conversion_to_512_byte_blocks} block_length = {block_length}");

    let info = SDCardInfo {
        internal_info: sd_card.clone(),
        reletave_card_address: rca,
        block_size: SD_BLOCK_SIZE.try_into().expect("To be on 64 bit system"),
        total_blocks: actual_block_number.into(),
    };

    return Result::Ok(info);
}

fn reset_sd_card(sd_card: &SDCardInfoInternal) -> Result<SDCardInfo, SDCardError> {
    software_reset_sd_card(&sd_card)?;
    enable_sd_card_interrupts(&sd_card)?;
    power_on_sd_card(&sd_card)?;
    set_sd_clock(&sd_card)?;
    // Is this needed, should this be here if so
    enable_timeouts(&sd_card)?;

    let info = send_sd_reset_commands(&sd_card)?;

    return Result::Ok(info);
}

// Determines if sending a command is currently valid
fn sending_command_valid(sd_card: &SDCardInfoInternal) -> Result<(), SDCardError> {
    check_no_errors(sd_card)?;
    let present_state_register_addr = (sd_card.base_address_register + 0x24) as *const u32;
    let present_state = unsafe { core::ptr::read_volatile(present_state_register_addr) };
    let inhibited_state = PresentState::CommandInhibitCmd | PresentState::CommandInhibitData;
    if inhibited_state.intersects(PresentState::from_bits_retain(present_state)) {
        return Result::Err(SDCardError::CommandInhibited);
    }
    if PresentState::CommandNotIssuedError.contains(PresentState::from_bits_retain(present_state)) {
        debug_println!("Present state = 0x{present_state:X}");
        return Result::Err(SDCardError::CommandStoppedDueToError);
    }
    return Result::Ok(());
}

fn check_no_errors(sd_card: &SDCardInfoInternal) -> Result<(), SDCardError> {
    let error_state_intr_addr = (sd_card.base_address_register + 0x32) as *const u16;
    let error_state = unsafe { core::ptr::read_volatile(error_state_intr_addr) };
    if error_state != 0 {
        debug_println!("Error detected 0x{error_state:x}");
        // TODO: Give better error
        return Result::Err(SDCardError::GenericSDError);
    }

    return Result::Ok(());
}

/// Bugs: Does not currently work with cmd 12 or 23 (gets response in wrong place)
/// Also Response type r7 does not seem to be documented
fn send_sd_command(
    sd_card: &SDCardInfoInternal,
    command_idx: u8,
    respone_type: SDResponseTypes,
    flags: CommandFlags,
) -> Result<SDCommandResponse, SDCardError> {
    assert!(command_idx < 64);
    let _ = sending_command_valid(sd_card)?;

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
        SDResponseTypes::R1 | SDResponseTypes::R5 | SDResponseTypes::R7 | SDResponseTypes::R6 => {
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
    sending_command_valid(sd_card)?;
    unsafe { core::ptr::write_volatile(command_register_addr, command) };
    check_no_errors(sd_card)?;
    let interrupt_status_register = (sd_card.base_address_register + 0x30) as *mut u16;
    for _ in 0..MAX_ITERATIONS {
        unsafe {
            let command_done = core::ptr::read_volatile(interrupt_status_register);
            if (command_done & 1) == 1 {
                core::ptr::write_volatile(interrupt_status_register, 1);
                return Result::Ok(determine_sd_card_response(&sd_card, respone_type));
            } else if command_done != 0 {
                debug_println!("Something happened 0x{command_done:X}");
            }
        }
    }
    check_no_errors(sd_card)?;
    return Result::Err(SDCardError::SDTimeout);
}

fn determine_sd_card_response(
    sd_card: &SDCardInfoInternal,
    respone_type: SDResponseTypes,
) -> SDCommandResponse {
    match respone_type {
        SDResponseTypes::R1b
        | SDResponseTypes::R1
        | SDResponseTypes::R3
        | SDResponseTypes::R4
        | SDResponseTypes::R5b
        | SDResponseTypes::R5
        | SDResponseTypes::R6 => {
            let response_register = (sd_card.base_address_register + 0x10) as *const u32;
            let response = unsafe { core::ptr::read_volatile(response_register) };
            return SDCommandResponse::Response32Bits(response);
        }
        SDResponseTypes::R2 => {
            let mut response: u128 = 0;
            let response_register = (sd_card.base_address_register + 0x10) as *const u32;
            let response_32_bits: u128 =
                unsafe { core::ptr::read_volatile(response_register).into() };
            response |= response_32_bits;
            let response_register = (sd_card.base_address_register + 0x14) as *const u32;
            let response_32_bits: u128 =
                unsafe { core::ptr::read_volatile(response_register).into() };
            response |= response_32_bits << 32;
            let response_register = (sd_card.base_address_register + 0x18) as *const u32;
            let response_32_bits: u128 =
                unsafe { core::ptr::read_volatile(response_register).into() };
            response |= response_32_bits << 64;
            let response_register = (sd_card.base_address_register + 0x1C) as *const u32;
            let response_32_bits: u128 =
                unsafe { core::ptr::read_volatile(response_register).into() };
            response |= response_32_bits << 96;
            return SDCommandResponse::Response128Bits(response);
        }
        _ => return SDCommandResponse::NoResponse,
    }
}

/// Reads data from a sd card
pub fn read_sd_card(sd_card: &SDCardInfo, block: u32) -> Result<[u8; 512], SDCardError> {
    let internal_info = &sd_card.internal_info;
    let block_size_register_addr = (internal_info.base_address_register + 0x4) as *mut u16;
    unsafe { core::ptr::write_volatile(block_size_register_addr, 0x200) };
    let block_count_register_addr = (internal_info.base_address_register + 0x6) as *mut u16;
    unsafe { core::ptr::write_volatile(block_count_register_addr, 1) };
    sending_command_valid(&internal_info)?;
    let argument_register_addr = (internal_info.base_address_register + 0x8) as *mut u32;
    unsafe { core::ptr::write_volatile(argument_register_addr, block * SD_BLOCK_SIZE) };
    let transfer_mode_register_adder = (internal_info.base_address_register + 0xC) as *mut u16;
    unsafe {
        core::ptr::write_volatile(
            transfer_mode_register_adder,
            TransferModeFlags::ReadToCard.bits(),
        )
    };

    // Send command
    send_sd_command(
        &internal_info,
        17,
        SDResponseTypes::R1,
        CommandFlags::DataPresentSelect,
    )?;

    let present_state_register_addr = (internal_info.base_address_register + 0x24) as *const u32;
    let mut finished = false;
    for _ in 0..MAX_ITERATIONS {
        let present_state = unsafe {
            PresentState::from_bits_retain(core::ptr::read_volatile(present_state_register_addr))
        };
        if PresentState::BufferReadEnable.intersects(present_state) {
            finished = true;
            break;
        }
        core::hint::spin_loop();
    }
    if !finished {
        debug_println!("Timedout");
        return Result::Err(SDCardError::SDTimeout);
    }

    let mut data = [0; 128];
    let buffer_data_port_reg_addr = (internal_info.base_address_register + 0x20) as *const u32;
    for i in 0..128 {
        data[i] = unsafe { core::ptr::read_volatile(buffer_data_port_reg_addr) };
    }

    let new_data = unsafe { core::mem::transmute(data) };
    return Result::Ok(new_data);
}

pub fn write_sd_card(sd_card: &SDCardInfo, block: u32, data: [u8; 512]) -> Result<(), SDCardError> {
    let internal_info = &sd_card.internal_info;
    let block_size_register_addr = (internal_info.base_address_register + 0x4) as *mut u16;
    unsafe { core::ptr::write_volatile(block_size_register_addr, 0x200) };
    let block_count_register_addr = (internal_info.base_address_register + 0x6) as *mut u16;
    unsafe { core::ptr::write_volatile(block_count_register_addr, 1) };
    sending_command_valid(internal_info)?;
    let argument_register_addr = (internal_info.base_address_register + 0x8) as *mut u32;
    unsafe { core::ptr::write_volatile(argument_register_addr, block * SD_BLOCK_SIZE) };
    let transfer_mode_register_adder = (internal_info.base_address_register + 0xC) as *mut u16;
    unsafe { core::ptr::write_volatile(transfer_mode_register_adder, 0) };

    // Send command
    send_sd_command(
        &internal_info,
        24,
        SDResponseTypes::R1,
        CommandFlags::DataPresentSelect,
    )?;

    let present_state_register_addr = (internal_info.base_address_register + 0x24) as *const u32;
    let mut finished = false;
    for _ in 0..MAX_ITERATIONS {
        let present_state = unsafe {
            PresentState::from_bits_retain(core::ptr::read_volatile(present_state_register_addr))
        };
        if PresentState::BufferWriteEnable.intersects(present_state) {
            finished = true;
            break;
        }
        core::hint::spin_loop();
    }
    if !finished {
        let present_state = unsafe { core::ptr::read_volatile(present_state_register_addr) };
        debug_println!("State = 0x{present_state:X}");
        return Result::Err(SDCardError::SDTimeout);
    }

    let data_32_bits: [u32; 128] = unsafe { core::mem::transmute(data) };
    let buffer_data_port_reg_addr = (internal_info.base_address_register + 0x20) as *mut u32;
    for i in 0..128 {
        unsafe {
            core::ptr::write_volatile(buffer_data_port_reg_addr, data_32_bits[i]);
        }
    }

    return Result::Ok(());
}
