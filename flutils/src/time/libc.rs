use std::{thread, time};

use futures::Future;

pub fn now() -> f64 {
    use chrono::Utc;
    Utc::now().timestamp_millis() as f64
}

pub fn block_on<F: Future<Output = ()>>(f: F) {
    futures::executor::block_on(f);
}

pub fn schedule_repeating<F>(cb: F)
where
    F: 'static + FnMut() + Send,
{
    let timer = timer::Timer::new();
    timer.schedule_repeating(chrono::Duration::seconds(1), cb);
}

pub async fn wait_ms(ms: u32) {
    thread::sleep(time::Duration::from_millis(ms.into()));
}
