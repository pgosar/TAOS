use alloc::{sync::Arc, vec::Vec};
use spin::Mutex;
use x86_64::structures::paging::OffsetPageTable;

use crate::debug_println;

use super::{
    mmio,
    pci::{read_config, DeviceInfo},
};

#[derive(Debug, Clone, Copy)]
struct XHCICapabilities {
    register_length: u8,
    version_number: u16,
    structural_paramaters_1: u32,
    structural_paramaters_2: u32,
    structural_paramaters_3: u32,
    capability_paramaters_1: u32,
    doorbell_offset: u32,
    runtime_register_space_offset: u32,
    capability_paramaters_2: u32,
}
struct XHCIInfo {
    address: u64,
}

const XHCI_CLASS_CODE: u8 = 0x0C;
const XHCI_SUB_CLASS_CODE: u8 = 0x03;
const XHCI_PROGRAMMING_INTERFACE: u8 = 0x30;

/// Finds the FIRST device that represents an XHCI device
pub fn find_xhci_inferface(
    devices: &Vec<Arc<Mutex<DeviceInfo>>>,
) -> Option<Arc<Mutex<DeviceInfo>>> {
    for possible_device in devices {
        let arc_device = possible_device.clone();
        let device = arc_device.lock();
        if device.class_code == XHCI_CLASS_CODE
            && device.subclass == XHCI_SUB_CLASS_CODE
            && device.programming_interface == XHCI_PROGRAMMING_INTERFACE
        {
            return Option::Some(possible_device.clone());
        }
    }
    Option::None
}

/// Initalizes an xhci_hub
pub fn initalize_xhci_hub(device: &Arc<Mutex<DeviceInfo>>, mapper: &mut OffsetPageTable) {
    let device_lock = device.clone();
    let xhci_device = device_lock.lock();
    let bar_0: u64 = (read_config(xhci_device.bus, xhci_device.device, 0, 0x10) & 0xFFFFFFF0).into(); 
    let bar_1: u64 = read_config(xhci_device.bus, xhci_device.device, 0, 0x14).into();
    debug_println!("Bar 0 = 0x{bar_0} Bar 1 = 0x{bar_1}");
    let full_bar = (bar_1 << 32) | bar_0;

    debug_println!("Full bar = 0x{full_bar:X}");
    let address = mmio::map_page_as_uncacheable(full_bar, mapper).unwrap();
    let _info = XHCIInfo { address };
    let capablities = get_host_controller_cap_regs(address);
    debug_println!("0x{address:X} {capablities:?}");
}

/// Determines the capablities of a given host controller
fn get_host_controller_cap_regs(address: u64) -> XHCICapabilities {
    let register_length_addr = (address) as *const u8;
    let register_length = unsafe { core::ptr::read_volatile(register_length_addr) };

    let version_no_addr = (address + 0x2) as *const u16;
    let version_no = unsafe { core::ptr::read_volatile(version_no_addr) };

    let hcs_params_1_addr = (address + 0x4) as *const u32;
    let hcs_params_1 = unsafe { core::ptr::read_volatile(hcs_params_1_addr) };
    let hcs_params_2_addr = (address + 0x8) as *const u32;
    let hcs_params_2 = unsafe { core::ptr::read_volatile(hcs_params_2_addr) };
    let hcs_params_3_addr = (address + 0xC) as *const u32;
    let hcs_params_3 = unsafe { core::ptr::read_volatile(hcs_params_3_addr) };

    let cap_params_1_addr = (address + 0x10) as *const u32;
    let cap_params_1 = unsafe { core::ptr::read_volatile(cap_params_1_addr) };

    let doorbell_addr = (address + 0x14) as *const u32;
    let doorbell_offset = unsafe { core::ptr::read_volatile(doorbell_addr) };
    let runtime_register_addr = (address + 0x18) as *const u32;
    let runtime_register_offset = unsafe { core::ptr::read_volatile(runtime_register_addr) };

    let cap_params_2_addr = (address + 0x18) as *const u32;
    let cap_params_2 = unsafe { core::ptr::read_volatile(cap_params_2_addr) };

    XHCICapabilities {
        register_length,
        version_number: version_no,
        structural_paramaters_1: hcs_params_1,
        structural_paramaters_2: hcs_params_2,
        structural_paramaters_3: hcs_params_3,
        capability_paramaters_1: cap_params_1,
        doorbell_offset,
        runtime_register_space_offset: runtime_register_offset,
        capability_paramaters_2: cap_params_2,
    }
}
