use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use limine::{
    request::{FramebufferRequest, SmpRequest},
    smp::{Cpu, RequestFlags},
    BaseRevision,
};

use crate::{
    constants::x2apic::CPU_FREQUENCY,
    events::{register_event_runner, run_loop},
    interrupts::{gdt, idt, x2apic},
    memory::{
        boot_frame_allocator::BootIntoFrameAllocator,
        frame_allocator::{GlobalFrameAllocator, FRAME_ALLOCATOR},
        heap, paging,
    },
    serial_println,
};

#[used]
#[link_section = ".requests"]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[link_section = ".requests"]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[link_section = ".requests"]
static SMP_REQUEST: SmpRequest = SmpRequest::new().with_flags(RequestFlags::X2APIC);

static BOOT_COMPLETE: AtomicBool = AtomicBool::new(false);
static CPU_COUNT: AtomicU64 = AtomicU64::new(0);

pub fn init() -> u32 {
    assert!(BASE_REVISION.is_supported());
    serial_println!("Booting BSP...");

    gdt::init(0);
    idt::init_idt(0);
    x2apic::init_bsp(CPU_FREQUENCY).expect("Failed to configure x2APIC");

    unsafe {
        *FRAME_ALLOCATOR.lock() = Some(GlobalFrameAllocator::Boot(BootIntoFrameAllocator::init()));
    }
    let mut mapper = unsafe { paging::init() };
    heap::init_heap(&mut mapper).expect("Failed to initialize heap");

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
    let bsp_id = smp_response.bsp_lapic_id();

    serial_println!("Detected {} CPU cores", cpu_count);

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

    register_event_runner(bsp_id);
    idt::enable();

    bsp_id
}

#[no_mangle]
unsafe extern "C" fn secondary_cpu_main(cpu: &Cpu) -> ! {
    CPU_COUNT.fetch_add(1, Ordering::SeqCst);
    gdt::init(cpu.id);
    idt::init_idt(cpu.id);
    x2apic::init_ap().expect("Failed to initialize core APIC");

    serial_println!("AP {} initialized", cpu.id);

    while !BOOT_COMPLETE.load(Ordering::SeqCst) {
        core::hint::spin_loop();
    }

    idt::enable();

    register_event_runner(cpu.id);
    serial_println!("AP {} entering event loop", cpu.id);
    run_loop(cpu.id)
}
