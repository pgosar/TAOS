#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use limine::request::{
    FramebufferRequest, HhdmRequest, MemoryMapRequest, RequestsEndMarker, RequestsStartMarker,
    SmpRequest,
};
use limine::response::MemoryMapResponse;
use limine::smp::Cpu;
use limine::BaseRevision;
use taos::interrupts::{gdt, idt};
use taos::memory::frameallocator::BootIntoFrameAllocator;
use taos::memory::memory;
use taos::{idle_loop, serial_println};
use x86_64::structures::paging::{Page, PageTable, Translate};
use x86_64::{
    structures::paging::{FrameAllocator, OffsetPageTable},
    VirtAddr,
};

#[used]
#[link_section = ".requests"]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[link_section = ".requests"]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[link_section = ".requests"]
static SMP_REQUEST: SmpRequest = SmpRequest::new();

#[used]
#[link_section = ".requests"]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

#[used]
#[link_section = ".requests"]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

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

    let memory_map_response: &MemoryMapResponse = MEMORY_MAP_REQUEST
        .get_response()
        .expect("Memory map request failed");
    let hhdm_response = HHDM_REQUEST.get_response().expect("HHDM request failed");

    let HHDM_OFFSET: u64 = hhdm_response.offset();

    let mut frame_allocator = unsafe { BootIntoFrameAllocator::init(&memory_map_response) };
    // new: initialize a mapper

    let mut mapper = unsafe { memory::init(VirtAddr::new(HHDM_OFFSET)) };
    let page = Page::containing_address(VirtAddr::new(0xb8000));

    let page2 = Page::containing_address(VirtAddr::new(0x201008));

    memory::create_example_mapping(page, &mut mapper, &mut frame_allocator);

    memory::create_example_mapping(page2, &mut mapper, &mut frame_allocator);

    let addresses = [
        // the identity-mapped vga buffer page
        0xb8000,
        // some code page
        0x201008,
        // some stack page
        0x0100_0020_1a10,
        // virtual address mapped to physical address 0
        HHDM_OFFSET,
    ];
    for &address in &addresses {
        let phys = mapper.translate_addr(VirtAddr::new(address));
        serial_println!("{:?} -> {:?}", VirtAddr::new(address), phys);
    }
    frame_allocator.allocate_frame();

    serial_println!("BSP entering idle loop");
    idle_loop();
}

#[no_mangle]
unsafe extern "C" fn secondary_cpu_main(cpu: &Cpu) -> ! {
    CPU_COUNT.fetch_add(1, Ordering::SeqCst);
    gdt::init(cpu.id);
    idt::init_idt(cpu.id);
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
