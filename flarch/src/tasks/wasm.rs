use std::future::Future;

pub fn now() -> f64 {
    use js_sys::Date;
    Date::now()
}

pub fn spawn_local<F: Future<Output = ()> + 'static>(f: F) {
    wasm_bindgen_futures::spawn_local(f);
}

#[cfg(not(feature = "node"))]
pub async fn wait_ms(ms: u64) {
    wasm_timer::Delay::new(core::time::Duration::from_millis(ms))
        .await
        .expect("Waiting for delay to finish");
}

#[allow(unused_imports)]
use wasm_bindgen::prelude::*;
#[cfg_attr(
    feature = "node",
    wasm_bindgen(
        inline_js = "module.exports.wait_ms = function(ms){ return new Promise((r) => setTimeout(r, ms));}"
    )
)]
#[cfg(feature = "node")]
extern "C" {
    pub async fn wait_ms(ms: u32);
}
