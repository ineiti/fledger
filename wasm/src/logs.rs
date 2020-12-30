use wasm_bindgen::prelude::*;

macro_rules! console_log {
    ($($t:tt)*) => (crate::logs::log(&format_args!($($t)*).to_string()))
}
macro_rules! console_warn {
    ($($t:tt)*) => (crate::logs::warn(&format_args!($($t)*).to_string()))
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    pub fn log(s: &str);
    #[wasm_bindgen(js_namespace = console)]
    pub fn warn(s: &str);
}

#[wasm_bindgen(inline_js = "export function wait_ms(ms) { return new Promise((r) => setTimeout(r, ms)); }")]
extern "C" {
    pub async fn wait_ms(ms: u32);
}
