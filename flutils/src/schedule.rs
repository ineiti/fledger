#[cfg(target_arch = "wasm32")]
pub fn schedule_repeating<F>(cb: F)
where
    F: 'static + FnMut() + Send,
{
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        pub fn setInterval(callback: JsValue, millis: u32) -> f64;
    }
    let ccb = Closure::wrap(Box::new(cb) as Box<dyn FnMut()>);
    setInterval(ccb.into_js_value(), 1000);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn schedule_repeating<F>(cb: F)
where
    F: 'static + FnMut() + Send,
{
    let timer = timer::Timer::new();
    timer.schedule_repeating(chrono::Duration::seconds(1), cb);
}
