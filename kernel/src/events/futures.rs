use alloc::sync::Arc;
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use crate::interrupts::x2apic;
use futures::task::ArcWake;

use super::{runner_timestamp, Event};

pub struct Sleep {
    pub target_timestamp: u64,
    pub event: Arc<Event>,
}

impl Sleep {
    pub fn new(target_timestamp: u64, event: Arc<Event>) -> Sleep {
        Sleep {
            target_timestamp,
            event,
        }
    }

    pub fn awake(&self) {
        self.event.clone().wake();
    }
}

impl Ord for Sleep {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.target_timestamp
            .cmp(&other.target_timestamp)
            .reverse()
            .then(self.event.eid.cmp(&other.event.eid))
    }
}

impl PartialOrd for Sleep {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Sleep {
    fn eq(&self, other: &Self) -> bool {
        self.target_timestamp == other.target_timestamp && self.event.eid == other.event.eid
    }
}

impl Eq for Sleep {}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        let cpuid = x2apic::current_core_id() as u32;
        let system_time = runner_timestamp(cpuid);

        if self.target_timestamp <= system_time {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
