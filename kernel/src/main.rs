#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use limine::request::{
    FramebufferRequest, HhdmRequest, MemoryMapRequest, RequestsEndMarker, RequestsStartMarker,
    SmpRequest,
};
use limine::response::MemoryMapResponse;
use limine::smp::{Cpu, RequestFlags};  // Combining both branches
use limine::BaseRevision;
use taos::constants::x2apic::CPU_FREQUENCY;  // Added from x2apic branch
use taos::interrupts::{gdt, idt, x2apic};  // Keeping both `idt` and `x2apic`
use taos::memory::{frame_allocator::BootIntoFrameAllocator, paging};  // Keeping `frame_allocator` and `paging`
use taos::{idle_loop, serial_println};
use x86_64::structures::paging::{Page, Translate};
use x86_64::VirtAddr;

extern crate alloc;
use alloc::boxed::Box;
use taos::memory::allocator;

#[used]
#[link_section = ".requests"]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[link_section = ".requests"]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[link_section = ".requests"]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[link_section = ".requests"]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

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
    x2apic::init_bsp(CPU_FREQUENCY).expect("Failed to configure x2APIC");

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

    idt::enable();

    // set up page tables
    // get hhdm offset
    let memory_map_response: &MemoryMapResponse = MEMORY_MAP_REQUEST
        .get_response()
        .expect("Memory map request failed");
    let hhdm_response = HHDM_REQUEST.get_response().expect("HHDM request failed");

    let hhdm_offset: VirtAddr = VirtAddr::new(hhdm_response.offset());

    // decide what frames we can allocate based on memmap
    let mut frame_allocator = unsafe { BootIntoFrameAllocator::init(memory_map_response) };

    let mut mapper = unsafe { paging::init(hhdm_offset) };

    // test mapping
    let page = Page::containing_address(VirtAddr::new(0xb8000));

    paging::create_mapping(page, &mut mapper, &mut frame_allocator);

    let addresses = [
        // the identity-mapped vga buffer page
        0xb8000, 0x201008,
    ];
    for &address in &addresses {
        let phys = mapper.translate_addr(VirtAddr::new(address));
        serial_println!("{:?} -> {:?}", VirtAddr::new(address), phys);
    }

    // testing that the heap allocation works
    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");
    let x: Box<i32> = Box::new(10);
    let y: Box<i32> = Box::new(20);
    let z: Box<i32> = Box::new(30);
    serial_println!(
        "Heap object allocated at: {:p}",
        Box::as_ref(&x) as *const i32
    );
    serial_println!(
        "Heap object allocated at: {:p}",
        Box::as_ref(&y) as *const i32
    );
    serial_println!(
        "Heap object allocated at: {:p}",
        Box::as_ref(&z) as *const i32
    );

    // should trigger page fault and panic
    //unsafe {
    //    *(0x201008 as *mut u64) = 42; // Guaranteed to page fault
    //}

    serial_println!("BSP entering idle loop");
    idle_loop();
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
    serial_println!("AP {} entering idle loop", cpu.id);
    idle_loop();
}

#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    serial_println!("Kernel panic: {}", info);
    idle_loop();
}
