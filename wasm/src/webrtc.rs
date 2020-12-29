use super::logs;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    match super::rtc_node::demo().await {
        Err(e) => console_warn!("Couldn't finish task: {:?}", e),
        Ok(_) => (),
    };
    Ok(())
}
