use js_sys::JsString;
use wasm_bindgen::JsValue;

/// Crate-wide error type for wasm boundary methods that return `Result<T, String>`.
#[derive(Debug)]
pub struct WasmError(pub String);

impl From<JsValue> for WasmError {
    fn from(e: JsValue) -> Self {
        WasmError(format!("{e:?}"))
    }
}

impl From<&str> for WasmError {
    fn from(s: &str) -> Self {
        WasmError(s.to_string())
    }
}

impl From<anyhow::Error> for WasmError {
    fn from(e: anyhow::Error) -> Self {
        WasmError(format!("{e:?}"))
    }
}

impl From<WasmError> for String {
    fn from(e: WasmError) -> Self {
        e.0
    }
}

impl From<WasmError> for JsString {
    fn from(e: WasmError) -> Self {
        e.0.into()
    }
}

impl From<WasmError> for JsValue {
    fn from(e: WasmError) -> Self {
        JsValue::from_str(&e.0)
    }
}
