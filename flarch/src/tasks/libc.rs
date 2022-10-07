use tokio::time::{sleep, Duration};

use futures::Future;

pub fn now() -> f64 {
    use chrono::Utc;
    Utc::now().timestamp_millis() as f64
}

pub fn spawn_local<F: Future<Output = ()> + 'static + Send>(f: F) {
    tokio::spawn(async { f.await });
}

pub async fn wait_ms(ms: u32) {
    sleep(Duration::from_millis(ms.into())).await;
}
