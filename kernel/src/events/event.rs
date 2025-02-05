extern crate alloc;
use alloc::collections::VecDeque;
use alloc::boxed::Box;

use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use core::{future::Future, pin::Pin};

pub struct Event {
  future: Pin<Box<dyn Future<Output = ()>>>
}

impl Event {
  pub fn init(future: impl Future<Output = ()> + 'static) -> Event {
    Event {
        future: Box::pin(future),
    }
  }

  fn poll(&mut self, context: &mut Context) -> Poll<()> {
    self.future.as_mut().poll(context)
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
        Poll::Pending => self.event_queue.push_back(event),
        Poll::Ready(()) => {}
      }
    }
  }

  pub fn enqueue(&mut self, event: Event) {
    self.event_queue.push_back(event);
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

