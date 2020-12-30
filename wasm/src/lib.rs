use wasm_bindgen::prelude::*;
use yew::prelude::*;

#[macro_use]
mod logs;
mod rest;
mod rtc_node;
mod webrtc;

struct Model {
    link: ComponentLink<Self>,
    log: String,
}

enum Msg {
    ClearNodes,
    SendID,
    Reset,
    Connect,
    WebRTCDone,
}

async fn wrap<F: std::future::Future>(f: F, done_cb: yew::Callback<F::Output>) {
    done_cb.emit(f.await);
}

async fn wrap_short<F: std::future::Future>(f: F) {
    f.await;
}

async fn rtc_demo() {
    match rest::demo().await {
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
            log: "".to_string(),
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::SendID => {
                wasm_bindgen_futures::spawn_local(wrap(
                    rtc_demo(),
                    self.link.callback(|_| Msg::WebRTCDone),
                ));
            }
            Msg::WebRTCDone => {}
            Msg::ClearNodes => {}
            Msg::Reset => {}
            Msg::Connect => {}
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
                        <button onclick=self.link.callback(|_| Msg::SendID)>{ "+1" }</button>
                        <p>{"log:"}{ log_str }</p>
                    </div>
                </div>
            </div>
        }
    }
}

#[wasm_bindgen(start)]
pub async fn run_app() {
    console_error_panic_hook::set_once();
    App::<Model>::new().mount_to_body();
    console_log!("starting app for now!");
}
