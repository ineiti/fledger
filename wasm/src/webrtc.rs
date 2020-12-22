use wasm_bindgen::prelude::*;
use super::logs;

#[wasm_bindgen]
pub async fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    match
    // super::example::get_example().await?;
    // super::peer_conn::example().await?;
    super::rtc_node::demo().await {
        Err(e) => console_warn!("Couldn't finish task: {:?}", e),
        Ok(_) => ()
    };
    Ok(())
}
