#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use limine::request::{FramebufferRequest, RequestsEndMarker, RequestsStartMarker, SmpRequest};
use limine::smp::{Cpu, RequestFlags};
use limine::BaseRevision;
use taos::interrupts::{gdt, idt, x2apic};
use taos::{idle_loop, serial_println};

#[used]
#[link_section = ".requests"]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[link_section = ".requests"]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[link_section = ".requests"]
static SMP_REQUEST: SmpRequest = SmpRequest::new().with_flags(RequestFlags::X2APIC);

#[used]
#[link_section = ".requests_start_marker"]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();

#[used]
#[link_section = ".requests_end_marker"]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

static BOOT_COMPLETE: AtomicBool = AtomicBool::new(false);
static CPU_COUNT: AtomicU64 = AtomicU64::new(0);

#[no_mangle]
extern "C" fn kmain() -> ! {
    assert!(BASE_REVISION.is_supported());

    serial_println!("Booting BSP...");

    gdt::init(0);
    idt::init_idt(0);

    x2apic::init_x2apic().expect("Failed to initialize x2apic");
    let ticks = x2apic::calibrate_apic_timer().expect("Failed to calibrate apic timer");
    serial_println!("{}", ticks);

    if let Some(framebuffer_response) = FRAMEBUFFER_REQUEST.get_response() {
        serial_println!("Found frame buffer");
        if let Some(framebuffer) = framebuffer_response.framebuffers().next() {
            for i in 0..100_u64 {
                let pixel_offset = i * framebuffer.pitch() + i * 4;
                unsafe {
                    *(framebuffer.addr().add(pixel_offset as usize) as *mut u32) = 0xFFFFFFFF;
                }
            }
        }
    }

    let smp_response = SMP_REQUEST.get_response().expect("SMP request failed");
    let cpu_count = smp_response.cpus().len() as u64;

    serial_println!("Detected {} CPU cores", cpu_count);

    let bsp_id = smp_response.bsp_lapic_id();
    for cpu in smp_response.cpus() {
        if cpu.id != bsp_id {
            cpu.goto_address.write(secondary_cpu_main);
        }
    }

    while CPU_COUNT.load(Ordering::SeqCst) < cpu_count - 1 {
        core::hint::spin_loop();
    }

    BOOT_COMPLETE.store(true, Ordering::SeqCst);
    serial_println!("All CPUs initialized");

    x2apic::configure_cpu_timer(32, 4, ticks).unwrap();
    idt::enable();
    serial_println!("BSP entering idle loop");
    idle_loop();
}

#[no_mangle]
unsafe extern "C" fn secondary_cpu_main(cpu: &Cpu) -> ! {
    CPU_COUNT.fetch_add(1, Ordering::SeqCst);
    gdt::init(cpu.id);
    idt::init_idt(cpu.id);
    /*let (apic_id, mode, ticks) = x2apic::init().expect("Failed to initialize the x2 APIC");
    serial_println!("ID: {}\nMode: {:#?}\nTicks: {}", apic_id, mode, ticks);*/

    serial_println!("AP {} initialized", cpu.id);

    while !BOOT_COMPLETE.load(Ordering::SeqCst) {
        core::hint::spin_loop();
    }

    idt::enable();
    serial_println!("AP {} entering idle loop", cpu.id);
    idle_loop();
}

#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    serial_println!("Kernel panic: {}", info);
    idle_loop();
}
