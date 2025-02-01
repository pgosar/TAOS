use x86_64::instructions::port::{
    self, PortGeneric, ReadOnlyAccess, ReadWriteAccess, WriteOnlyAccess,
};

use crate::debug_println;

const MAX_PCI_DEVICES: usize = 16;
const CONFIG_ADDRESS_BUS: u16 = 0xCF8;
const CONFIG_DATA_BUS: u16 = 0xCFC;
pub const COMMAND_INTERRUPT_DISABLE: u16 = 0x1 << 10;
pub const COMMAND_BUS_MASTER: u16 = 0x1 << 2;
pub const COMMAND_MEMORY_SPACE: u16 = 0x1 << 1;
pub const COMMAND_IO_SPACE: u16 = 0x1;

pub struct DeviceInfo {
    pub bus: u8,
    pub device: u8,
    pub device_id: u16,
    pub vendor_id: u16,
    pub status: u16,
    pub command: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub programming_interface: u8,
    pub revision_id: u8,
    pub built_in_self_test: u8,
    pub header_type: u8,
    pub latency_timer: u8,
    pub cache_line_size: u8,
}

pub struct AllDeviceInfo {
    pub devices_connected: usize,
    pub device_info: [Option<DeviceInfo>; MAX_PCI_DEVICES],
}

fn get_pci_addres(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    assert!(offset % 4 == 0);
    assert!(function < 8);
    assert!(device < 32);

    let bus_extended: u32 = bus.into();
    let device_extended: u32 = device.into();
    let function_extended: u32 = function.into();
    let offset_extended: u32 = offset.into();

    return 1 << 31
        | bus_extended << 16
        | device_extended << 11
        | function_extended << 8
        | offset_extended;
}

/// Reads from pci config and returns the result into a u32. Note: device must be 
/// less than 32, function must be less than 8, and offset must be a multiple 
/// of 4.
pub unsafe fn read_config(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let address = get_pci_addres(bus, device, function, offset);
    let mut address_port: PortGeneric<u32, WriteOnlyAccess> =
        port::PortGeneric::new(CONFIG_ADDRESS_BUS);
    address_port.write(address);

    let mut config_port: PortGeneric<u32, ReadOnlyAccess> = port::PortGeneric::new(CONFIG_DATA_BUS);
    return config_port.read();
}

/// Writes the given command into the command register. It is recommended
/// to get the old value of command and set and unset the appropate bits
/// from the command, as some bits are read only.
pub unsafe fn write_command(bus: u8, device: u8, function: u8, command: u16) {
    let address = get_pci_addres(bus, device, function, 0x4);
    let mut address_port: PortGeneric<u32, WriteOnlyAccess> =
        port::PortGeneric::new(CONFIG_ADDRESS_BUS);
    address_port.write(address);

    let mut config_port: PortGeneric<u32, ReadWriteAccess> =
        port::PortGeneric::new(CONFIG_DATA_BUS);
    let mut old_data: u32 = config_port.read();
    old_data &= 0xFFFF0000;
    old_data |= <u16 as Into<u32>>::into(command);

    config_port.write(old_data);
}

fn device_connected(bus: u8, device: u8) -> Option<DeviceInfo> {
    let mut config_word = unsafe { read_config(bus, device, 0, 0) };
    let device_id: u16 = (config_word >> 16).try_into().unwrap();
    let vendor_id: u16 = (config_word & 0x0000FFFF).try_into().unwrap();

    if vendor_id == 0xFFFF {
        return Option::None;
    }

    config_word = unsafe { read_config(bus, device, 0, 4) };
    let status: u16 = (config_word >> 16).try_into().unwrap();
    let command: u16 = (config_word & 0x0000FFFF).try_into().unwrap();

    config_word = unsafe { read_config(bus, device, 0, 8) };
    let class_code: u8 = (config_word >> 24).try_into().unwrap();
    let subclass: u8 = ((config_word & 0x00FF0000) >> 16).try_into().unwrap();
    let programming_interface: u8 = ((config_word & 0x0000FF00) >> 8).try_into().unwrap();
    let revision_id: u8 = (config_word & 0x000000FF).try_into().unwrap();

    config_word = unsafe { read_config(bus, device, 0, 12) };
    let built_in_self_test: u8 = (config_word >> 24).try_into().unwrap();
    let header_type: u8 = ((config_word & 0x00FF0000) >> 16).try_into().unwrap();
    let latency_timer: u8 = ((config_word & 0x0000FF00) >> 8).try_into().unwrap();
    let cache_line_size: u8 = (config_word & 0x000000FF).try_into().unwrap();

    let device_info = DeviceInfo {
        bus: bus,
        device: device,
        device_id: device_id,
        vendor_id: vendor_id,
        status: status,
        command: command,
        class_code: class_code,
        subclass: subclass,
        programming_interface: programming_interface,
        revision_id: revision_id,
        built_in_self_test: built_in_self_test,
        header_type: header_type,
        latency_timer: latency_timer,
        cache_line_size: cache_line_size,
    };
    return Option::Some(device_info);
}

fn print_pci_info(device: &DeviceInfo) {
    debug_println!("----------");
    debug_println!("bus = {}", { device.bus });
    debug_println!("device = {}", { device.device });
    debug_println!("device_id = 0x{:X}", { device.device_id });
    debug_println!("vendor_id = 0x{:X}", { device.vendor_id });
    debug_println!("status = 0x{:X}", { device.status });
    debug_println!("class_code = 0x{:X}", { device.class_code });
    debug_println!("subclass = 0x{:X}", { device.subclass });
    debug_println!("programming_interface = 0x{:X}", {
        device.programming_interface
    });
}

pub fn walk_pci_bus() -> AllDeviceInfo {
    let mut connected_devices = 0;
    let mut bus: u16 = 0;
    let mut devices: [Option<DeviceInfo>; MAX_PCI_DEVICES] =
        [const { Option::None }; MAX_PCI_DEVICES];
    while bus < 256 {
        let mut device: u8 = 0;
        while device < 32 {
            match device_connected(bus.try_into().unwrap(), device) {
                None => (),
                Some(device_info) => {
                    print_pci_info(&device_info);
                    devices[connected_devices] = Some(device_info);
                    connected_devices += 1;
                }
            }
            device += 1
        }
        bus += 1
    }
    return AllDeviceInfo {
        devices_connected: connected_devices,
        device_info: devices,
    };
}
