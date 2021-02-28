#![recursion_limit = "1024"]

use yew::services::IntervalService;
use common::node::ext_interface::Logger;
use std::sync::Arc;
use std::sync::Mutex;

use std::time::Duration;

use wasm_bindgen::prelude::*;
use yew::prelude::*;

use js_sys::Date;
use regex::Regex;
use wasm_lib::{
    logger::{LoggerOutput, NodeLogger},
    node::{start, MyDataStorage},
};
use web_sys::{console, window};

use common::node::Node;

fn log_1(s: &str) {
    console::log_1(&JsValue::from(s));
}

fn log_2(s: &str, t: String) {
    console::log_2(&JsValue::from(s), &JsValue::from(t));
}

#[cfg(not(feature = "local"))]
const URL: &str = "wss://signal.fledg.re";

#[cfg(feature = "local")]
const URL: &str = "ws://localhost:8765";

struct Model {
    link: ComponentLink<Self>,
    node: Option<Arc<Mutex<Node>>>,
    log_str: Arc<Mutex<String>>,
    counter: u32,
    show_reset: bool,
    logger: NodeLogger,
}

enum Msg {
    UpdateLog,
    Reset,
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
        log_1("setting panic hook");
        console_error_panic_hook::set_once();
        Model::set_localstorage();

        let (logger, node_logger) = LoggerOutput::new();
        Model::start_node(node_logger.clone(), &link);
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
            counter: 0,
            show_reset: false,
            logger: node_logger,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::UpdateLog => {
                self.counter += 1;
                if self.counter % 15 == 0 {
                    self.node_list();
                    self.node_ping();
                }
            }
            Msg::Node(res_node) => {
                match res_node {
                    Ok(node) => self.node = Some(Arc::new(Mutex::new(node))),
                    Err(e) => {
                        self.logger.error(&format!("Couldn't create node: {}", e.as_string().unwrap()));
                        self.show_reset = true;
                    }
                }
            }
            Msg::Reset => {
                Model::set_config("");
                self.show_reset = false;
                Model::start_node(self.logger.clone(), &self.link);
            }
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
        let log = self.log_str.lock().unwrap();
        let reset_style = match self.show_reset {
            false => "display: none;",
            true => "",
        };
        html! {
            <div class="main">
                <div class="ui">
                    <div>
                        <ul>
                            <li>{self.connection_state((*log).clone())}</li>
                            <li>{self.nodes_connected()}</li>
                            <li>{self.nodes_reachable()}</li>
                        </ul>
                        <button style={reset_style} onclick=self.link.callback(|_| Msg::Reset)>{ "Reset Config" }</button>
                        <pre class="wrap" id="log">{"log:"}
                        { log }</pre>
                    </div>
                </div>
            </div>
        }
    }
}

impl Model {
    fn start_node(logger: Box<dyn Logger>, link: &ComponentLink<Model>){
        wasm_bindgen_futures::spawn_local(wrap(
            start(logger, URL),
            link.callback(|n: Result<Node, JsValue>| Msg::Node(n)),
        ));
    }

    fn set_config(data: &str) {
        if let Err(err) = Node::set_config(Box::new(MyDataStorage {}), &data) {
            log_2("Got error while saving config:", err);
        }
    }

    fn set_localstorage() {
        if let Ok(loc) = window().unwrap().location().href() {
            log_2("Location is", loc.clone());
            if loc.contains("#") {
                let reg = Regex::new(r".*?#").unwrap();
                let data_enc = reg.replace(&loc, "");
                if data_enc != "" {
                    log_1("Setting data");
                    if let Ok(data) = urlencoding::decode(&data_enc) {
                        Model::set_config(&data);
                    }
                }
            }
        }
    }

    fn connection_state(&self, log: String) -> String {
        format!(
            "State of connection: {}",
            match self.node {
                Some(_) =>
                    if log.contains("Announce") {
                        "Connected"
                    } else {
                        "Connecting"
                    },
                None => "Unknown",
            }
        )
    }

    fn nodes_connected(&self) -> String {
        return format!(
            "Other nodes connected: {}",
            if let Some(n) = self.node_copy() {
                if let Ok(list) = n.lock().unwrap().network.get_list() {
                    list.len().to_string()
                } else {
                    "0".to_string()
                }
            } else {
                "N/A".to_string()
            }
        );
    }

    fn nodes_reachable(&self) -> &str {
        return "Nodes reachable: N/A";
    }

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
                    log_2("Couldn't get list:", e);
                }
            });
        }
    }

    fn node_ping(&self) {
        if let Some(n) = self.node_copy() {
            wasm_bindgen_futures::spawn_local(async move {
                let mut node = n.lock().unwrap();
                let str = Date::new_0().to_iso_string().as_string().unwrap();
                if let Err(e) = node.ping(&str).await {
                    log_2("Couldn't ping node:", e);
                }
            });
        }
    }
}

#[wasm_bindgen(start)]
pub async fn run_app() {
    console_error_panic_hook::set_once();
    App::<Model>::new().mount_to_body();
    log_1("starting app for now!");
}
