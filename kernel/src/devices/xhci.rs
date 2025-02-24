use alloc::{sync::Arc, vec::Vec};
use spin::Mutex;
use x86_64::{
    structures::paging::{OffsetPageTable, Translate},
    VirtAddr,
};

use super::pci::{read_config, DeviceInfo};

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
    let bar_0: u64 = read_config(xhci_device.bus, xhci_device.device, 0, 0x10).into();
    let bar_1: u64 = read_config(xhci_device.bus, xhci_device.device, 0, 0x14).into();
    let full_bar = bar_0 << 32 | bar_1;

    let offset = mapper.phys_offset().as_u64();
    let mut offset_bar = full_bar + offset;
    let translate_result = mapper.translate(VirtAddr::new(offset_bar));
    let info = XHCIInfo { address: full_bar };
}
