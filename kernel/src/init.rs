use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use limine::{
    request::SmpRequest,
    smp::{Cpu, RequestFlags},
    BaseRevision,
};
use x86_64::VirtAddr;

use crate::{
    debug, devices,
    events::{register_event_runner, run_loop},
    interrupts::{self, idt},
    logging, memory, trace,
};

use crate::{
    constants::processes::BINARY,
    events::schedule,
    memory::paging::HHDM_REQUEST,
};

extern crate alloc;
use crate::{
    processes::process::{create_process, print_process_table, run_process_ring3, PROCESS_TABLE},
    serial_println,
};

#[used]
#[link_section = ".requests"]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[link_section = ".requests"]
static SMP_REQUEST: SmpRequest = SmpRequest::new().with_flags(RequestFlags::X2APIC);

static BOOT_COMPLETE: AtomicBool = AtomicBool::new(false);
static CPU_COUNT: AtomicU64 = AtomicU64::new(0);

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

    bsp_id
}

#[no_mangle]
unsafe extern "C" fn secondary_cpu_main(cpu: &Cpu) -> ! {
    CPU_COUNT.fetch_add(1, Ordering::SeqCst);
    interrupts::init(cpu.id);
    memory::init(cpu.id);
    devices::init(cpu.id);
    logging::init(cpu.id);

    debug!("AP {} initialized", cpu.id);

    while !BOOT_COMPLETE.load(Ordering::SeqCst) {
        core::hint::spin_loop();
    }

    idt::enable();

    register_event_runner(cpu.id);
    if cpu.id == 1 {
    let hhdm_response = HHDM_REQUEST.get_response().expect("HHDM request failed");

    let hhdm_base: VirtAddr = VirtAddr::new(hhdm_response.offset());
    let mut mapper = crate::memory::MAPPER.lock();
    unsafe {
        // this must be the issue then
        // it seems as if its trying to schedule on both cores
        // do yk where the secondary cpu stuff went that was previously in main?
        // oh good point, lemme find out
        // init.rs
        serial_println!("Current CPU ID {}", 1);
        let pid = create_process(BINARY, &mut *mapper, hhdm_base);
        print_process_table(&PROCESS_TABLE);
        schedule(1, run_process_ring3(pid), 0, pid)
    };
}
    debug!("AP {} entering event loop", cpu.id);
    run_loop(cpu.id)
}

fn wake_cores() -> u32 {
    let smp_response = SMP_REQUEST.get_response().expect("SMP request failed");
    let cpu_count = smp_response.cpus().len() as u64;
    let bsp_id = smp_response.bsp_lapic_id();

    trace!("Detected {} CPU cores", cpu_count);

    for cpu in smp_response.cpus() {
        if cpu.id != bsp_id {
            cpu.goto_address.write(secondary_cpu_main);
        }
    }

    while CPU_COUNT.load(Ordering::SeqCst) < cpu_count - 1 {
        core::hint::spin_loop();
    }

    BOOT_COMPLETE.store(true, Ordering::SeqCst);

    debug!("All CPUs initialized");
    bsp_id
}
