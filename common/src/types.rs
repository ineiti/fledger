use futures::Future;
#[cfg(target_arch = "wasm32")]
pub fn now() -> f64 {
    use js_sys::Date;
    Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn now() -> f64 {
    use chrono::Utc;
    Utc::now().timestamp_millis() as f64
}

#[cfg(target_arch = "wasm32")]
pub fn block_on<F: Future<Output = ()> + 'static>(f: F) {
    wasm_bindgen_futures::spawn_local(f);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn block_on<F: Future<Output = ()>>(f: F) {
    futures::executor::block_on(f);
}

pub fn type_to_string<T>(_: &T) -> String {
    format!("{}", std::any::type_name::<T>())
}
