extern crate alloc;
use alloc::collections::btree_map::BTreeMap;
use alloc::{boxed::Box, collections::btree_set::BTreeSet};
use alloc::sync::Arc;
use spin::mutex::Mutex;
use spin::rwlock::RwLock;

use core::{future::Future, pin::Pin, sync::atomic::AtomicU64, sync::atomic::Ordering};

use crossbeam_queue::ArrayQueue;

use crate::constants::events::NUM_EVENT_PRIORITIES;

mod event;
mod event_runner;
pub mod futures;

type SyncFuture = Mutex<Pin<Box<dyn Future<Output = ()> + 'static + Send>>>;
type EventQueue = Arc<ArrayQueue<Arc<Event>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct EventId(u64);

impl EventId {
  fn init() -> Self {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    EventId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
  }
}

struct Event {
  eid: EventId,
  future: SyncFuture,
  rewake_queue: EventQueue,
  priority: usize
}

#[derive(Debug)]
struct EventRunner {
  event_queues: [EventQueue; NUM_EVENT_PRIORITIES],
  rewake_queue: EventQueue,
  pending_events: RwLock<BTreeSet<u64>>,
}

static EVENT_RUNNERS: RwLock<BTreeMap<u32, RwLock<EventRunner>>> = RwLock::new(BTreeMap::new());

pub unsafe fn run_loop(cpuid: u32) -> ! {
  let runners = EVENT_RUNNERS.read();
  let runner = runners.get(&cpuid).expect("No runner found").as_mut_ptr();

  (*runner).run_loop()
}

pub fn schedule(cpuid: u32, future: impl Future<Output = ()> + 'static + Send, priority_level: usize) {
  let runners = EVENT_RUNNERS.read();
  let mut runner = runners.get(&cpuid).expect("No runner found").write();

  runner.schedule(future, priority_level);
}

pub fn register_event_runner(cpuid: u32) {
  let runner = EventRunner::init();
  let mut write_lock = EVENT_RUNNERS.write();

  write_lock.insert(cpuid, RwLock::new(runner));
}