use common::node::Node;
use yew::services::IntervalService;

use std::sync::Arc;
use std::sync::Mutex;

use std::time::Duration;

use wasm_bindgen::prelude::*;
use yew::prelude::*;

// use rest::demo;
// use web_rtc::demo;
use lib::node::start;

const URL: &str = "ws://localhost:8765";

struct Model {
    link: ComponentLink<Self>,
    node: Option<Arc<Mutex<Node>>>,
    log_str: Arc<Mutex<String>>,
}

enum Msg {
    UpdateLog,
    List,
    Ping,
    Node(Result<Node, JsValue>),
}

async fn wrap<F: std::future::Future>(f: F, done_cb: yew::Callback<F::Output>) {
    done_cb.emit(f.await);
}

async fn wrap_short<F: std::future::Future>(f: F) {
    f.await;
}

impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        console_log!("setting panic hook");
        console_error_panic_hook::set_once();

        let (logger, node_logger) = LoggerOutput::new();
        wasm_bindgen_futures::spawn_local(wrap(
            start(Box::new(node_logger), URL),
            link.callback(|n: Result<Node, JsValue>| Msg::Node(n)),
        ));
        wasm_bindgen_futures::spawn_local(wrap_short(LoggerOutput::listen(
            logger.ch,
            Arc::clone(&logger.str),
        )));
        let _ = Box::leak(Box::new(IntervalService::spawn(
            Duration::from_secs(1),
            link.callback(|_| Msg::UpdateLog),
        )));
        Self {
            link,
            node: None,
            log_str: Arc::clone(&logger.str),
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::UpdateLog => {}
            Msg::Node(res_node) => {
                if let Ok(node) = res_node {
                    self.node = Some(Arc::new(Mutex::new(node)));
                }
            }
            Msg::List => self.node_list(),
            Msg::Ping => self.node_ping(),
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
        let str = self.log_str.lock().unwrap();
        html! {
            <div class="main">
                <div class="ui">
                    <div>
                        <button onclick=self.link.callback(|_| Msg::List)>{ "List Nodes" }</button>
                        <button onclick=self.link.callback(|_| Msg::Ping)>{ "Ping Nodes" }</button>
                        <pre class="wrap">{"log:"}
                        { str }</pre>
                    </div>
                </div>
            </div>
        }
    }
}

impl Model {
    fn node_copy<'a>(&self) -> Option<Arc<Mutex<Node>>> {
        if let Some(n) = &self.node {
            Some(Arc::clone(&n))
        } else {
            None
        }
    }

    // fn node_connect(&self){
    //     if let Some(n) = self.node_copy(){
    //         wasm_bindgen_futures::spawn_local(async move {
    //             let mut node = n.lock().unwrap();
    //             node.connect().await;
    //         });
    //     }
    // }

    fn node_list(&self) {
        if let Some(n) = self.node_copy() {
            wasm_bindgen_futures::spawn_local(async move {
                let mut node = n.lock().unwrap();
                if let Err(e) = node.list().await {
                    console_warn!("Couldn't get list: {}", e);
                }
            });
        }
    }

    fn node_ping(&self) {
        if let Some(n) = self.node_copy() {
            wasm_bindgen_futures::spawn_local(async move {
                let mut node = n.lock().unwrap();
                if let Err(e) = node.ping().await{
                    console_warn!("Couldn't ping node: {}", e);
                }
            });
        }
    }
}

#[wasm_bindgen(start)]
pub async fn run_app() {
    console_error_panic_hook::set_once();
    App::<Model>::new().mount_to_body();
    console_log!("starting app for now!");
}
