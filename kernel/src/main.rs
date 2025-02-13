#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use limine::request::{
    FramebufferRequest, HhdmRequest, KernelAddressRequest, MemoryMapRequest, RequestsEndMarker,
    RequestsStartMarker, SmpRequest,
};
use limine::response::MemoryMapResponse;
use limine::smp::{Cpu, RequestFlags};
use limine::BaseRevision;
use taos::constants::{processes::BINARY, x2apic::CPU_FREQUENCY};
use taos::events::futures::print_nums_after_rand_delay;
use taos::events::{register_event_runner, run_loop, schedule};
use taos::interrupts::{gdt, idt, x2apic};
use taos::processes::process::{
    create_process, print_process_table, run_process_ring3, PROCESS_TABLE,
};
use x86_64::structures::paging::{Page, PhysFrame, Size4KiB, Translate};
use x86_64::VirtAddr;

extern crate alloc;
use alloc::boxed::Box;

use taos::filesys::block::memory::MemoryBlockDevice;
use taos::filesys::fat16::Fat16;
use taos::filesys::{FileSystem, SeekFrom};
use taos::{
    idle_loop,
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
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[link_section = ".requests"]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

#[used]
#[link_section = ".requests"]
static SMP_REQUEST: SmpRequest = SmpRequest::new().with_flags(RequestFlags::X2APIC);

#[used]
#[link_section = ".requests"]
static KERNEL_ADDRESS_REQUEST: KernelAddressRequest = KernelAddressRequest::new();

#[used]
#[link_section = ".requests_start_marker"]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();

#[used]
#[link_section = ".requests_end_marker"]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

static BOOT_COMPLETE: AtomicBool = AtomicBool::new(false);
static MEMORY_SETUP: AtomicBool = AtomicBool::new(false);
static CPU_COUNT: AtomicU64 = AtomicU64::new(0);

extern "C" {
    static _kernel_end: u64;
}

// ASYNC
async fn test_sum(start: u64) -> u64 {
    let mut sum: u64 = start;
    const MAX: u64 = 10000000;
    for i in 0..MAX {
        sum += i;
        if i == MAX / 2 {
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
    let tv2 = test_sum(arg1 * 2).await;
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

    let kernel_address_response = KERNEL_ADDRESS_REQUEST
        .get_response()
        .expect("Kernel Address request failed");

    let physical_kernel_address: u64 = kernel_address_response.physical_base();
    let virtual_kernel_address: u64 = kernel_address_response.virtual_base();
    serial_println!(
        "Kernel physical base address: {:#X}, virtual base address: {:#X}",
        physical_kernel_address,
        virtual_kernel_address
    );

    unsafe {
        serial_println!("virtual kernel end address: {:#X}", _kernel_end);
    }

    let physical_kernel_end =
        unsafe { ((_kernel_end) - virtual_kernel_address) + physical_kernel_address };

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
            physical_kernel_address,
            physical_kernel_end,
        )));
    }

    let mut mapper = unsafe { paging::init(hhdm_offset) };
    use x86_64::registers::model_specific::{Efer, EferFlags};

    unsafe {
        // Must be done after enabling long mode + paging
        Efer::update(|flags| {
            flags.insert(EferFlags::NO_EXECUTE_ENABLE);
        });
    }

    // testing that the heap allocation works
    heap::init_heap(&mut mapper).expect("heap initialization failed");

    let device = Box::new(MemoryBlockDevice::new(512, 512));

    // Format the filesystem
    let mut fs = Fat16::format(device).expect("Failed to format filesystem");

    // Test directory operations
    fs.create_dir("/test_dir")
        .expect("Failed to create directory");
    fs.create_dir("/test_dir/subdir")
        .expect("Failed to create subdirectory");

    // Create a test file
    fs.create_file("/test_dir/test.txt")
        .expect("FailedBlockDevice to create file");
    let fd = fs
        .open_file("/test_dir/test.txt")
        .expect("Failed to open file");

    // Write test data
    let test_data = b"Hello, TaOS FAT16!";
    let written = fs.write_file(fd, test_data).expect("Failed to write");
    assert_eq!(written, test_data.len(), "Write length mismatch");

    fs.seek_file(fd, SeekFrom::Start(0))
        .expect("Failed to seek to start");
    let mut read_buf = [0u8; 32];
    let read = fs.read_file(fd, &mut read_buf).expect("Failed to read");
    assert_eq!(read, test_data.len(), "Read length mismatch");
    assert_eq!(&read_buf[..read], test_data, "Read data mismatch");

    // List directory contents
    let entries = fs.read_dir("/test_dir").expect("Failed to read directory");
    assert!(!entries.is_empty(), "Directory should not be empty");
    assert_eq!(entries.len(), 2, "Directory should have exactly 2 entries"); // subdir and test.txt

    // Check file metadata
    let metadata = fs
        .metadata("/test_dir/test.txt")
        .expect("Failed to get metadata");
    assert_eq!(metadata.size, test_data.len() as u64, "File size mismatch");
    assert!(!metadata.is_dir, "Should not be a directory");

    // Test partial reads
    fs.seek_file(fd, SeekFrom::Start(7))
        .expect("Failed to seek");
    let mut partial_buf = [0u8; 4];
    let read = fs.read_file(fd, &mut partial_buf).expect("Failed to read");
    assert_eq!(read, 4, "Partial read length mismatch");
    assert_eq!(&partial_buf[..read], b"TaOS", "Partial read data mismatch");

    // // Close file and test removal
    fs.close_file(fd);

    fs.remove_file("/test_dir/test.txt")
        .expect("Failed to remove file");
    fs.remove_dir("/test_dir/subdir")
        .expect("Failed to remove subdirectory");
    fs.remove_dir("/test_dir")
        .expect("Failed to remove directory");

    // Verify root is empty
    let root_entries = fs.read_dir("/").expect("Failed to read root directory");
    assert_eq!(root_entries.len(), 0, "Root directory should be empty");

    serial_println!("FAT16 filesystem test passed!");

    MEMORY_SETUP.store(true, Ordering::SeqCst);

    register_event_runner(bsp_id);

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
    paging::create_mapping(new_page, &mut mapper, None);

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
    //schedule(bsp_id, print_nums_after_rand_delay(0x1332), 3, 0);
    //schedule(bsp_id, print_nums_after_rand_delay(0x532), 2, 0);
    //schedule(bsp_id, test_event_two_blocks(400), 0, 0);
    //schedule(bsp_id, test_event(100), 3, 0);

    // // Try giving something to CPU 2 (note this is not how it'll be done for real, just a test)
    //schedule(1, test_event(353), 1, 0);

    // This loads in the binary and creates a process
    let pid = create_process(BINARY, &mut mapper, hhdm_offset);
    print_process_table(&PROCESS_TABLE);
    unsafe { schedule(bsp_id, run_process_ring3(pid), 0, pid) };

    serial_println!("BSP entering event loop");
    unsafe { run_loop(bsp_id) };
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

    // To avoid constant page faults
    while !MEMORY_SETUP.load(Ordering::SeqCst) {
        core::hint::spin_loop();
    }

    let x: Box<i32> = Box::new(10);
    serial_println!(
        "AP {} Heap object allocated at: {:p}",
        cpu.id,
        Box::as_ref(&x) as *const i32
    );

    // ASYNC
    register_event_runner(cpu.id);

    //schedule(cpu.id, test_event(200), 2, 0);

    serial_println!("AP {} entering event loop", cpu.id);
    run_loop(cpu.id)
}

#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    serial_println!("Kernel panic: {}", info);
    idle_loop();
}
