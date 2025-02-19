//! Kernel Initialization
//!
//! Handles the initialization of kernel subsystems and CPU cores.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use limine::{
    request::SmpRequest,
    smp::{Cpu, RequestFlags},
    BaseRevision,
};

use crate::{
    constants,
    constants::processes::BINARY,
    debug, devices,
    events::{register_event_runner, run_loop, schedule},
    interrupts::{self, idt, x2apic::X2ApicManager},
    logging, memory,
    processes::process::{create_process, print_process_table, run_process_ring3, PROCESS_TABLE},
    serial_println, trace,
};

extern crate alloc;

/// Limine base revision request
#[used]
#[link_section = ".requests"]
static BASE_REVISION: BaseRevision = BaseRevision::new();

/// Symmetric Multi-Processing (SMP) request with x2APIC support
#[used]
#[link_section = ".requests"]
static SMP_REQUEST: SmpRequest = SmpRequest::new().with_flags(RequestFlags::X2APIC);

/// Flag indicating completion of boot process
/// Used to synchronize AP initialization
static BOOT_COMPLETE: AtomicBool = AtomicBool::new(false);

/// Counter tracking number of initialized CPUs
static CPU_COUNT: AtomicU64 = AtomicU64::new(0);

/// Initializes kernel subsystems for the Bootstrap Processor (BSP)
///
/// # Returns
/// * `u32` - The BSP's LAPIC ID
pub fn init() -> u32 {
    assert!(BASE_REVISION.is_supported());
    interrupts::init(0);
    memory::init(0);
    devices::init(0);
    // Should be kept after devices in case logging gets complicated
    // Right now log writes to serial, but if it were to switch to VGA, this would be important
    logging::init(0);

    debug!("Waking cores");
    let bsp_id = wake_cores();

    register_event_runner(bsp_id);
    idt::enable();

    unsafe {
        serial_println!("Current CPU ID {}", 1);
        let pid = create_process(BINARY);
        print_process_table(&PROCESS_TABLE);
        schedule(1, run_process_ring3(pid), 0, pid)
    };

    bsp_id
}

/// Entry point for Application Processors (APs)
///
/// # Arguments
/// * `cpu` - CPU information structure from Limine
///
/// # Safety
/// This function is unsafe because it:
/// - Is called directly by the bootloader
/// - Performs hardware initialization
/// - Must never return
#[no_mangle]
unsafe extern "C" fn secondary_cpu_main(cpu: &Cpu) -> ! {
    CPU_COUNT.fetch_add(1, Ordering::SeqCst);
    interrupts::init(cpu.id);
    memory::init(cpu.id);
    logging::init(cpu.id);

    debug!("AP {} initialized", cpu.id);

    // Wait for all cores to complete initialization
    while !BOOT_COMPLETE.load(Ordering::SeqCst) {
        core::hint::spin_loop();
    }

    register_event_runner(cpu.id);
    idt::enable();

    debug!("AP {} entering event loop", cpu.id);
    run_loop(cpu.id)
}

/// Initializes secondary CPU cores
///
/// # Returns
/// * `u32` - The BSP's LAPIC ID
fn wake_cores() -> u32 {
    let smp_response = SMP_REQUEST.get_response().expect("SMP request failed");
    let cpu_count = smp_response.cpus().len() as u64;
    let bsp_id = smp_response.bsp_lapic_id();

    trace!("Detected {} CPU cores", cpu_count);

    // Set entry point for each AP
    // for cpu in smp_response.cpus() {
    //     if cpu.id != bsp_id {
    //         cpu.goto_address.write(secondary_cpu_main);
    //     }
    // }

    // // Wait for all APs to initialize
    // while CPU_COUNT.load(Ordering::SeqCst) < cpu_count - 1 {
    //     core::hint::spin_loop();
    // }

    BOOT_COMPLETE.store(true, Ordering::SeqCst);

    debug!("All CPUs initialized");
    bsp_id
}