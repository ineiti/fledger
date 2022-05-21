use tokio::time::{self, sleep, Duration};

use futures::Future;

pub fn now() -> f64 {
    use chrono::Utc;
    Utc::now().timestamp_millis() as f64
}

pub fn block_on<F: Future<Output = ()> + 'static + Send>(f: F) {
    tokio::spawn(async { f.await });
}

pub fn schedule_repeating<F>(mut cb: F)
where
    F: 'static + FnMut() + Send,
{
    tokio::spawn( async move {
        let mut seconds = time::interval(Duration::from_secs(1));
        loop {
            seconds.tick().await;
            cb();
        }
    });
}

pub async fn wait_ms(ms: u32) {
    sleep(Duration::from_millis(ms.into())).await;
}
