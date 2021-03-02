use wasm_bindgen::prelude::*;

pub fn logit(){}

#[macro_export]
macro_rules! console_log {
    ($($t:tt)*) => (super::logs::log(&format_args!($($t)*).to_string()))
}
#[macro_export]
macro_rules! console_warn {
    ($($t:tt)*) => (super::logs::warn(&format_args!($($t)*).to_string()))
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    pub fn log(s: &str);
    #[wasm_bindgen(js_namespace = console)]
    pub fn warn(s: &str);
}

#[cfg_attr(feature = "node", wasm_bindgen(
    inline_js = "module.exports.wait_ms = function(ms){ return new Promise((r) => setTimeout(r, ms));}"
))]
#[cfg_attr(not(feature = "node"), wasm_bindgen(
    inline_js = "export function wait_ms(ms){ return new Promise((r) => setTimeout(r, ms));}"
))]
extern "C" {
    pub async fn wait_ms(ms: u32);
}
