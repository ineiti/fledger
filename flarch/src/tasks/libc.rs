use std::{
    pin::Pin,
    task::{Context, Poll},
};

use tokio::time::{self, sleep, Duration, Instant};

use futures::{Future, Stream};

/// Returns the milliseconds since 1/1/1970.
pub fn now() -> i64 {
    use chrono::Utc;
    Utc::now().timestamp_millis() as i64
}

pub fn spawn_local_nosend<F: Future<Output = ()> + 'static>(_: F) {
    panic!("Cannot spawn NoSend in libc");
}

/// Spawns the given method on the local scheduler.
pub fn spawn_local<F: Future<Output = ()> + 'static + Send>(f: F) {
    tokio::spawn(async { f.await });
}

/// Waits for dur.
pub async fn wait(dur: Duration) {
    sleep(dur).await;
}

/// Waits for ms milliseconds before returning.
pub async fn wait_ms(ms: u64) {
    wait(Duration::from_millis(ms)).await;
}

/// Interval stream that will send the expected time in regular intervals.
/// It simulates tokio::timer::Interval, but doesn't return an `Instant`, as
/// this is not available on wasm.
pub struct Interval {
    interval: time::Interval,
    next: i64,
    dur: i64,
}

impl Interval {
    /// Creates a new stream of Interval starting at next_millis and firing every
    /// dur.
    pub fn new(next_millis: i64, dur: Duration) -> Self {
        let next = Instant::now()
            .checked_add(Duration::from_millis((next_millis - now()) as u64))
            .unwrap();
        Self {
            interval: time::interval_at(next, dur),
            next: next_millis,
            dur: dur.as_millis() as i64,
        }
    }

    /// Creates a new stream of Interval starting now and firing every dur.
    pub fn new_interval(dur: Duration) -> Self {
        Self::new(now(), dur)
    }
}

impl Stream for Interval {
    type Item = i64;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.interval.poll_tick(cx) {
            Poll::Ready(_) => {
                let this = self.next;
                self.next += self.dur;
                Poll::Ready(Some(this))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
