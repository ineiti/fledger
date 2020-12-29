use wasm_bindgen::prelude::*;
use yew::prelude::*;

mod logs;
mod webrtc;
mod rest;
mod rtc_node;

struct Model {
    link: ComponentLink<Self>,
    value: i64,
    log: String,
}

enum Msg {
    AddOne,
    WebRTCDone,
}

async fn wrap<F: std::future::Future>(f: F, done_cb: yew::Callback<F::Output>) {
    done_cb.emit(f.await);
}

async fn wrap_short<F: std::future::Future>(f: F) {
    f.await;
}

async fn rtc_demo() {
    match rtc_node::demo().await {
        Err(e) => console_warn!("Couldn't finish task: {:?}", e),
        Ok(_) => (),
    };
}

impl Component for Model {
    type Message = Msg;
    type Properties = ();
    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        wasm_bindgen_futures::spawn_local(wrap_short(rtc_demo()));
        Self {
            link,
            value: 0,
            log: "".to_string(),
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::AddOne => {
                self.value += 1;
                wasm_bindgen_futures::spawn_local(wrap(
                    rtc_demo(),
                    self.link.callback(|_| Msg::WebRTCDone),
                ));
            }
            Msg::WebRTCDone => self.value += 10,
        }
        true
    }

    fn change(&mut self, _props: Self::Properties) -> ShouldRender {
        // Should only return "true" if new properties are different to
        // previously received properties.
        // This component has no properties so we will always return "false".
        false
    }

    fn view(&self) -> Html {
        let log_str = &self.log;
        html! {
            <div class="main">
                <div class="ui">
                    <div>
                        <button onclick=self.link.callback(|_| Msg::AddOne)>{ "+1" }</button>
                        <p>{ self.value }</p>
                        <p>{"log:"}{ log_str }</p>
                    </div>
                </div>
            </div>
        }
    }
}

// #[macro_use]
#[wasm_bindgen(start)]
pub async fn run_app() {
    console_error_panic_hook::set_once();
    App::<Model>::new().mount_to_body();
    console_log!("starting app for now!");
}
