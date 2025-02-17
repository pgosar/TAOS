#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(taos::test_runner)]
#![reexport_test_harness_main = "test_main"]

use limine::request::{RequestsEndMarker, RequestsStartMarker};
use taos::{
    constants::processes::BINARY,
    events::{run_loop, schedule},
    memory::paging::HHDM_REQUEST,
};

extern crate alloc;
use taos::{
    debug,
    processes::process::{create_process, print_process_table, run_process_ring3, PROCESS_TABLE},
    serial_println,
};
use x86_64::VirtAddr;

#[used]
#[link_section = ".requests_start_marker"]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();

#[used]
#[link_section = ".requests_end_marker"]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

#[no_mangle]
extern "C" fn _start() -> ! {
    let bsp_id = taos::init::init();
    #[cfg(test)]
    test_main();

    debug!("BSP entering event loop");

    unsafe { run_loop(bsp_id) }
}

#[cfg(not(test))]
#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    serial_println!("Kernel panic: {}", info);
    taos::idle_loop();
}

#[cfg(test)]
#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    taos::test_panic_handler(info);
}
