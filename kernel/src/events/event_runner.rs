use super::{Event, EventId, EventQueue, EventRunner};

use alloc::{collections::{btree_set::BTreeSet, vec_deque::VecDeque}, sync::Arc};
use futures::task::waker_ref;
use spin::rwlock::RwLock;
use x86_64::instructions::interrupts;

use core::{
    future::Future,
    task::{Context, Poll},
};

use crate::constants::events::NUM_EVENT_PRIORITIES;

impl EventRunner {
    pub fn init() -> EventRunner {
        EventRunner {
            event_queues: core::array::from_fn(|_| RwLock::new(VecDeque::new())),
            rewake_queue: Arc::new(RwLock::new(VecDeque::new())),
            pending_events: RwLock::new(BTreeSet::new()),
            current_event: None,
        }
    }

    pub fn run_loop(&mut self) -> ! {
        loop {
            loop {
                if self.have_pending_events() {
                    break;
                }

                self.current_event = self.next_event();

                let event = self
                    .current_event
                    .as_ref()
                    .expect("Have pending events, but empty waiting queues.");

                if self.contains_event(event.eid) {
                    let waker = waker_ref(event);
                    let mut context: Context<'_> = Context::from_waker(&waker);

                    let mut future_guard = event.future.lock();

                    let ready: bool = future_guard.as_mut().poll(&mut context) != Poll::Pending;

                    drop(future_guard);

                    if !ready {
                        Self::enqueue(&self.event_queues[event.priority], event.clone());
                    } else {
                        let mut write_lock = self.pending_events.write();
                        write_lock.remove(&event.eid.0);
                    }
                }

                self.current_event = None;
            }

            // TODO do a lil work-stealing

            interrupts::enable_and_hlt();
        }
    }

    // Schedules an event with a specified priority level [0, NUM_EVENT_PRIORITIES)
    pub fn schedule(
        &mut self,
        future: impl Future<Output = ()> + 'static + Send,
        priority_level: usize,
        pid: u32,
    ) {
        if priority_level >= NUM_EVENT_PRIORITIES {
            panic!("Invalid event priority: {}", priority_level);
        } else {
            let event = Arc::new(Event::init(
                future,
                self.rewake_queue.clone(),
                priority_level,
                pid,
            ));

            Self::enqueue(&self.event_queues[priority_level], event.clone());


            let mut write_lock = self.pending_events.write();

            write_lock.insert(event.eid.0);

        }
    }

    pub fn current_running_event(&self) -> Option<&Arc<Event>> {
        self.current_event.as_ref()
    }

    fn have_pending_events(&self) -> bool {
        self.pending_events.read().is_empty()
    }

    fn contains_event(&self, eid: EventId) -> bool {
        self.pending_events.read().contains(&eid.0)
    }

    fn try_pop(queue: &EventQueue) -> Option<Arc<Event>> {
        queue.write().pop_front()
    }

    fn enqueue(queue: &EventQueue, event: Arc<Event>){
        queue.write().push_back(event);
    }

    fn next_event(&mut self) -> Option<Arc<Event>> {
        let rewake = Self::try_pop(&self.rewake_queue);
        if rewake.is_some() {
            rewake
        } else {
            let mut event = None;

            for i in 0..NUM_EVENT_PRIORITIES {
                event = Self::try_pop(&self.event_queues[i]);
                if event.is_some() {
                    break;
                }
            }

            event
        }
    }
}
