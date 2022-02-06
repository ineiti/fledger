use core::time::Duration;
use wasm_bindgen_test::*;

#[wasm_bindgen_test]
async fn test_wait_ms() {
    console_error_panic_hook::set_once();

    wasm_logger::init(wasm_logger::Config::new(log::Level::Trace));

    log::debug!("Running test");
    wait_ms(100).await;
    wait_ms(100).await;
    panic!("Show threads");
}

async fn wait_ms(ms: u32) {
    // let promise = Promise::new(&mut |yes, _| {
    //     let win = window().unwrap();
    //     win.set_timeout_with_callback_and_timeout_and_arguments_0(&yes, ms)
    //         .unwrap();
    // });
    // let js_fut = JsFuture::from(promise);
    // js_fut.await?;
    wasm_timer::Delay::new(Duration::new(0, ms * 1000000))
        .await
        .expect("Waiting for delay to finish");
}

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
