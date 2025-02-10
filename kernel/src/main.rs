#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use limine::request::{
    FramebufferRequest, HhdmRequest, MemoryMapRequest, RequestsEndMarker, RequestsStartMarker,
    SmpRequest,
};
use limine::response::MemoryMapResponse;
use limine::smp::{Cpu, RequestFlags};
use limine::BaseRevision;
use taos::constants::x2apic::CPU_FREQUENCY;
use taos::interrupts::{gdt, idt, x2apic};
use x86_64::structures::paging::{Page, PhysFrame, Size4KiB, Translate};
use x86_64::VirtAddr;

extern crate alloc;
use alloc::boxed::Box;

use taos::{
    idle_loop,
    memory::{
        boot_frame_allocator::BootIntoFrameAllocator,
        frame_allocator::{GlobalFrameAllocator, FRAME_ALLOCATOR},
        heap, paging,
    },
    serial_println,
    events::event
};

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

 // ASYNC
async fn test_sum(start: u64) -> u64 {
    let mut sum: u64 = start;
    const MAX: u64 = 10000000;
    for i in 0..MAX {
        sum += i;
        if i == MAX/2 {
            serial_println!("Halfway through long event");
        }
    }
    sum
}

async fn test_event(arg1: u64) {
    let tv = test_sum(arg1).await;
    serial_println!("Long event result: {}", tv);
}

async fn test_event_two_blocks(arg1: u64) {
    let tv = test_sum(arg1).await;
    let tv2 = test_sum(arg1*2).await;
    serial_println!("Long events results: {} {}", tv, tv2);
}

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

    // create a basic frame allocator and set it globally
    unsafe {
        *FRAME_ALLOCATOR.lock() = Some(GlobalFrameAllocator::Boot(BootIntoFrameAllocator::init(
            memory_map_response,
        )));
    }

    let mut mapper = unsafe { paging::init(hhdm_offset) };

    // test mapping
    let page = Page::containing_address(VirtAddr::new(0xb8000));

    paging::create_mapping(page, &mut mapper);

    let addresses = [
        // the identity-mapped vga buffer page
        0xb8000, 0x201008,
    ];
    for &address in &addresses {
        let phys = mapper.translate_addr(VirtAddr::new(address));
        serial_println!("{:?} -> {:?}", VirtAddr::new(address), phys);
    }

    // testing that the heap allocation works
    heap::init_heap(&mut mapper).expect("heap initialization failed");
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

    let alloc_dealloc_addr = VirtAddr::new(0x12515);
    let new_page: Page<Size4KiB> = Page::containing_address(alloc_dealloc_addr);
    serial_println!("Mapping a new page");
    paging::create_mapping(new_page, &mut mapper);

    let phys = mapper
        .translate_addr(alloc_dealloc_addr)
        .expect("Translation failed");
    // A downside of the current approach is that it is difficult to get the
    // specific methods that are a part of any allocator. Here is an example
    // on how to do this.
    {
        let mut alloc = FRAME_ALLOCATOR.lock();
        match *alloc {
            Some(GlobalFrameAllocator::Bitmap(ref mut bitmap_alloc)) => {
                serial_println!(
                    "{:?} -> {:?}, and the frame is {}",
                    alloc_dealloc_addr,
                    phys,
                    bitmap_alloc.is_frame_used(PhysFrame::containing_address(phys))
                );
            }
            _ => panic!("Bitmap alloc expected here"),
        }
    }

    serial_println!("Now unmapping the page");
    paging::remove_mapping(new_page, &mut mapper);
    match mapper.translate_addr(alloc_dealloc_addr) {
        Some(phys_addr) => {
            serial_println!("Mapping still exists at physical address: {:?}", phys_addr);
        }
        None => {
            serial_println!("Translation failed, as expected");
        }
    }

    {
        let alloc = FRAME_ALLOCATOR.lock();
        match *alloc {
            Some(GlobalFrameAllocator::Boot(ref _boot_alloc)) => {
                serial_println!("Boot frame allocator in use");
            }
            Some(GlobalFrameAllocator::Bitmap(ref _bitmap_alloc)) => {
                serial_println!("Bitmap frame allocator in use");
            }
            _ => panic!("Unknown frame allocator"),
        }
    }

    // ASYNC
    let mut runner = event::EventRunner::init();
    runner.schedule(event::print_nums_after_rand_delay(0x1332));

    runner.schedule(event::print_nums_after_rand_delay(0x532));
    runner.schedule(test_event_two_blocks(400));
    runner.schedule(test_event(100));

    serial_println!("BSP entering event loop");
    runner.run_loop();

    // serial_println!("BSP entering idle loop");
    // idle_loop();
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
