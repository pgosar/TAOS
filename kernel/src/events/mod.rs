extern crate alloc;
use alloc::{boxed::Box, collections::btree_set::BTreeSet};
use alloc::sync::Arc;

use core::{future::Future, pin::Pin, sync::atomic::AtomicU64, sync::atomic::Ordering};

use spin::Mutex;

use crossbeam_queue::ArrayQueue;

use crate::constants::events::NUM_EVENT_PRIORITIES;

mod event;
pub mod event_runner;
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

pub struct EventRunner {
  event_queues: [EventQueue; NUM_EVENT_PRIORITIES],
  rewake_queue: EventQueue,
  pending_events: BTreeSet<u64>
}