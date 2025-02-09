extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

// process counter must be thread-safe
static NEXT_PID: AtomicU32 = AtomicU32::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    New,
    Ready,
    Running,
    Blocked,
    Terminated,
}

#[derive(Debug)]
pub struct PCB {
    pub pid: u32,
    state: ProcessState,
    registers: [u64; 32],
    stack_pointer: u64,
    program_counter: u64,
}

// global process table must be thread-safe
lazy_static::lazy_static! {
    pub static ref PROCESS_TABLE: Mutex<BTreeMap<u32, Arc<PCB>>> = Mutex::new(BTreeMap::new());
}

pub fn create_process() -> Arc<PCB> {
    let pid = NEXT_PID.fetch_add(1, Ordering::SeqCst);

    let process = Arc::new(PCB {
        pid,
        state: ProcessState::New,
        registers: [0; 32],
        stack_pointer: 0,
        program_counter: 0,
    });

    // Insert into process table
    PROCESS_TABLE.lock().insert(pid, Arc::clone(&process));

    process
}
