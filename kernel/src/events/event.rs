extern crate alloc;
use alloc::boxed::Box;
use alloc::sync::Arc;

use core::task::{Context, Poll, Waker};
use core::{future::Future, pin::Pin};
use core::sync::atomic::{AtomicU64, Ordering};

use spin::Mutex;

use futures::future;
use futures::task::{waker_ref, ArcWake};

use crossbeam_queue::ArrayQueue;

use crate::constants::events::MAX_EVENTS;
use crate::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct EventId(u64);

impl EventId {
  fn init() -> Self {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    EventId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
  }
}

type SyncFuture = Mutex<Pin<Box<dyn Future<Output = ()> + 'static + Send>>>;
type EventQueue = Arc<ArrayQueue<Arc<Event>>>;

struct Event {
  eid: EventId,
  future: SyncFuture,
  event_queue: EventQueue
}

impl Event {
  fn init(future: impl Future<Output = ()> + 'static + Send, event_queue: EventQueue) -> Event {
    Event {
        eid: EventId::init(),
        future: Mutex::new(Box::pin(future)),
        event_queue: event_queue
    }
  }
}

pub struct EventRunner {
  event_queue: EventQueue,
}

impl ArcWake for Event {
    fn wake_by_ref(arc: &Arc<Self>) {
      // let r: Result<(), Arc<Event>> = arc.event_queue.push(arc.clone());
      // match r {
      //   Err(_) => {serial_println!("Event queue full!")}
      //   Ok(_) => {serial_println!("Awaken {:?}", arc.eid)},
      // }
      // serial_println!("Awaken {}", arc.eid.0);
    }
}

impl EventRunner {
  pub fn init() -> EventRunner {
    EventRunner {
      event_queue: Arc::new(ArrayQueue::new(MAX_EVENTS)),
    }
  }

  pub fn run(&mut self) {
    while let Some(event) = self.event_queue.pop() {
      let waker = waker_ref(&event);
      let mut context: Context<'_> = Context::from_waker(&waker);

      let mut future_guard = event.future.lock();

      let ready: bool = match future_guard.as_mut().poll(&mut context) {
        Poll::Pending => {
          serial_println!("Pending {:?}", event.eid); 
          false
        },
        Poll::Ready(()) => {
          serial_println!("Ready {:?}", event.eid); 
          true}
      };

      drop(future_guard);

      if !ready {
        let r: Result<(), Arc<Event>> = self.event_queue.push(event.clone());
        match r {
          Err(_) => {serial_println!("Event queue full!")}
          Ok(_) => (),
        }
      }
    }
  }

  pub fn schedule(&mut self, future: impl Future<Output = ()> + 'static + Send) {
    let r = self.event_queue.push(
      Arc::new(Event::init(future, self.event_queue.clone()))
    );
    match r {
      Err(_) => {serial_println!("Event queue full!")}
      Ok(_) => (),
    }
  }
}

// BELOW FOR TESTING/DEMONSTRATION PURPOSES

struct RandomFuture {
  prob: f64,
  rng: SmallRng,
  waker: Option<Waker>
}

impl RandomFuture {
  fn new(prob: f64, seed: u64) -> Self {
      RandomFuture {
        prob: prob, 
        rng: SmallRng::seed_from_u64(seed), 
        waker: None // Waker is created upon polling Pending
      }
  }
}

use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;

impl Future for RandomFuture {
  type Output = ();

  // Required method
  fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    let prob: f64 = self.prob;
    let res = self.rng.gen_bool(prob);
    
    serial_println!("RandPoll: {}", res);
    let poll = match res {
      true => {
        Poll::Ready(())
      },
      false => {
        self.waker = Some(cx.waker().clone());
        Poll::Pending
      }
    };

    match &self.waker {
      Some(waker) => {waker.wake_by_ref();}
      None => ()
    };
    poll
  }
}

async fn rand_delay(seed: u32) -> u32 {
  serial_println!("Awaiting random delay");
  let foo = RandomFuture::new(0.4, seed as u64);
  foo.await;
  seed
}

pub async fn print_nums_after_rand_delay(seed: u32) -> () {
  let res= future::join(rand_delay(seed), rand_delay(seed*2)).await;

  serial_println!("Results: {} {}", res.0, res.1);
}