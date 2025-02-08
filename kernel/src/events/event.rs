extern crate alloc;
use alloc::collections::VecDeque;
use alloc::boxed::Box;
use alloc::sync::Arc;

use alloc::task::Wake;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use core::{future::Future, pin::Pin};
use core::sync::atomic::{AtomicU64, Ordering};

use futures::future;

use crate::prelude::*;

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
  future: Pin<Box<dyn Future<Output = ()>>>,
}

impl Event {
  fn init(future: impl Future<Output = ()> + 'static) -> Event {
    Event {
        eid: EventId::init(),
        future: Box::pin(future)
    }
  }

  fn poll(&mut self, context: &mut Context) -> Poll<()> {
    serial_println!("Polled {:?}", self.eid);
    self.future.as_mut().poll(context)
  }
}

impl Wake for Event {
    fn wake(self: Arc<Self>) {
        todo!()
    }
}

pub struct EventRunner {
  event_queue: VecDeque<Event>,
}

impl EventRunner {
  pub fn init() -> EventRunner {
    EventRunner {
      event_queue: VecDeque::new(),
    }
  }

  pub fn run(&mut self) {
    while let Some(mut event) = self.event_queue.pop_front() {
      let waker = get_waker();
      let mut context = Context::from_waker(&waker);
      match event.poll(&mut context) {
        Poll::Pending => {serial_println!("Pending"); self.event_queue.push_back(event)},
        Poll::Ready(()) => {serial_println!("Ready");}
      }
    }
  }

  pub fn schedule(&mut self, future: impl Future<Output = ()> + 'static) {
    self.event_queue.push_back(Event::init(future));
  }
}

fn raw_waker() -> RawWaker {
  fn nop(_: *const()) {}
  fn clone(_: *const()) -> RawWaker {
    raw_waker()
  }

  let vtable = &RawWakerVTable::new(clone, nop, nop, nop);
  RawWaker::new(0 as *const(), vtable)
}

fn get_waker() -> Waker {
  unsafe {
    Waker::from_raw(raw_waker())
  }
}

struct RandomFuture {
  prob: f64,
  rng: SmallRng
}

impl RandomFuture {
  fn new(prob: f64, seed: u64) -> Self {
      RandomFuture {prob: prob, rng: SmallRng::seed_from_u64(seed)}
  }
}

use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;
use rand::RngCore;

impl Future for RandomFuture {
type Output = ();

// Required method
fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
  let prob: f64 = self.prob;
  let res = self.rng.gen_bool(prob);
  serial_println!("{}", res);
  match res {
    true => {
      cx.waker().clone().wake();
      Poll::Ready(())
    },
    false => Poll::Pending
  }
}
}

async fn rand_delay(arg1: u32) -> u32 {
  serial_println!("Awaiting random delay");
  let foo = RandomFuture::new(0.1, 0x1331);  //chosen by fair dice roll
  foo.await;
  arg1
}

pub async fn print_nums_after_rand_delay() -> () {
  let res= future::join(rand_delay(1), rand_delay(2)).await;

  serial_println!("Results: {} {}", res.0, res.1);
}