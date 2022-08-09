use wasm_bindgen::prelude::*;
use std::future::Future;

pub fn now() -> f64 {
    use js_sys::Date;
    Date::now()
}

pub fn block_on<F: Future<Output = ()> + 'static>(f: F) {
    wasm_bindgen_futures::spawn_local(f);
}

pub fn schedule_repeating<F>(cb: F)
where
    F: 'static + FnMut(),
{
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        pub fn setInterval(callback: JsValue, millis: u32) -> f64;
    }
    let ccb = Closure::wrap(Box::new(cb) as Box<dyn FnMut()>);
    setInterval(ccb.into_js_value(), 1000);
}

#[cfg(not(feature = "node"))]
pub async fn wait_ms(ms: u64){
    wasm_timer::Delay::new(core::time::Duration::from_millis(ms))
    .await
    .expect("Waiting for delay to finish");
}

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
