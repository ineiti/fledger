use futures::Stream;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::sync::oneshot::channel;

pub mod time {
    pub use std::time::*;
    pub use wasmtimer::tokio::*;
}

/// Returns the milliseconds since 1/1/1970 as i64.
pub fn now() -> i64 {
    use js_sys::Date;
    Date::now() as i64
}

/// Spawns the future on the local schedule.
pub fn spawn_local<F: Future<Output = ()> + 'static>(f: F) {
    wasm_bindgen_futures::spawn_local(f);
}

/// Spawns the future on the local schedule.
pub fn spawn_local_nosend<F: Future<Output = ()> + 'static>(f: F) {
    wasm_bindgen_futures::spawn_local(f);
}

/// Schedules a method in the background to run after dur.
pub fn schedule_once<F>(cb: F, dur: Duration)
where
    F: 'static + FnMut(),
{
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        pub fn setTimeout(callback: JsValue, millis: f64) -> f64;
    }
    let ccb = Closure::wrap(Box::new(cb) as Box<dyn FnMut()>);
    setTimeout(ccb.into_js_value(), dur.as_millis() as f64);
}

/// Interval stream that will send the expected time in regular intervals.
/// It simulates tokio::timer::Interval, but doesn't return an `Instant`, as
/// this is not available on wasm.
pub struct Interval {
    next: i64,
    dur: Duration,
}

impl Interval {
    /// Creates a new stream of Interval starting at next_millis and firing every
    /// dur.
    pub fn new(next_millis: i64, dur: Duration) -> Self {
        Self {
            next: next_millis,
            dur,
        }
    }

    /// Creates a new stream of Interval starting now and firing every dur.
    pub fn new_interval(dur: Duration) -> Self {
        Self::new(now(), dur)
    }
}

impl Stream for Interval {
    /// The time in milliseconds when the stream fired.
    type Item = i64;

    /// Checks if the duration is already expired and returns Poll::Ready with the current time.
    /// If the duration is not expired, wakes the poll up when it should expire.
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.next < 0 {
            return Poll::Pending;
        }

        let now = now();
        if now >= self.next {
            let this = self.next;
            self.next += self.dur.as_millis() as i64;
            Poll::Ready(Some(this))
        } else {
            let next = self.next - now;

            let waker = cx.waker().clone();
            schedule_once(
                move || {
                    waker.clone().wake();
                },
                Duration::from_millis(next as u64),
            );
            Poll::Pending
        }
    }
}

/// Waits for ms milliseconds before returning.
pub async fn wait_ms(millis: u64) {
    wait(Duration::from_millis(millis)).await;
}

/// Waits for dur before returning.
pub async fn wait(dur: Duration) {
    let (tx, rx) = channel::<()>();
    let mut tx_opt = Some(tx);
    schedule_once(
        move || {
            tx_opt.take().map(|tx| {
                tx.send(())
                    .err()
                    .map(|e| log::trace!("Couldn't send in wait_ms: {e:?}"))
            });
        },
        dur,
    );
    rx.await
        .err()
        .map(|e| log::trace!("While waiting on signal: {e:?}"));
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::stream::StreamExt;

    #[wasm_bindgen_test::wasm_bindgen_test]
    async fn test_interval() {
        web_sys::console::log_1(&"hello".into());
        let mut interv = Interval::new_interval(Duration::from_millis(100));
        for i in 0..10 {
            web_sys::console::log_1(&format!("{i}: {:?}", interv.next().await).into());
            if i < 7 {
                wait_ms(i * 20).await;
            }
        }
    }
}
