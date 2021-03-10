#![recursion_limit = "1024"]

use common::node::{config::NodeInfo, ext_interface::Logger};
use std::sync::Arc;
use std::sync::Mutex;
use yew::services::IntervalService;

use std::time::Duration;

use wasm_bindgen::prelude::*;
use yew::prelude::*;

use js_sys::Date;
use regex::Regex;
use wasm_lib::{
    logger::{LoggerOutput, NodeLogger},
    storage_logs::LocalStorage,
    web_rtc_setup::WebRTCConnectionSetupWasm,
    web_socket::WebSocketWasm,
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
    no_contact_yet: bool,
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
        console_error_panic_hook::set_once();
        Model::set_localstorage();

        let (logger, node_logger) = LoggerOutput::new();
        Model::node_start(node_logger.clone(), &link);
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
            no_contact_yet: true,
            logger: node_logger,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::UpdateLog => {
                if let Some(n) = self.node_copy() {
                    if self.no_contact_yet {
                        let mut node = n.lock().unwrap();
                        let l = node.get_list();
                        self.logger.info(&format!("List length is: {:?}", l));
                        if l.len() > 0 {
                            self.no_contact_yet = false;
                            self.node_ping();
                        }
                    }
                }
                self.process();
                self.counter += 1;
                if self.counter % 15 == 0 {
                    self.node_list();
                    self.node_ping();
                }
            }
            Msg::Node(res_node) => match res_node {
                Ok(node) => {
                    self.logger.info("Got node");
                    self.node = Some(Arc::new(Mutex::new(node)));
                }
                Err(e) => {
                    self.logger
                        .error(&format!("Couldn't create node: {}", e.as_string().unwrap()));
                    self.show_reset = true;
                }
            },
            Msg::Reset => {
                Model::set_config("");
                self.show_reset = false;
                Model::node_start(self.logger.clone(), &self.link);
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
                <h1>{"Fledger Web Node"}</h1>
                <p>{"This is a full node running in the web browser. Instead of having to invest
                in big hardware, fleger is light enough that you can participate using a browser.
                The mined Mana can be used to run smart contracts, store data, or use the
                re-encryption service."}</p>
                <p>{"For more information, see the documentation: "}
                <a href={"https://fledg.re/doc/index.html"} target={"other"}>{"Fledger - the blockchain that could"}</a>
                </p>
                <div class="ui">
                    <ul>
                        <li>{"Our node: "}{self.describe()}</li>
                        <li>{self.connection_state((*log).clone())}</li>
                        <li>{self.nodes_connected()}</li>
                        <li>{self.nodes_reachable()}</li>
                    </ul>
                    <button style={reset_style} onclick=self.link.callback(|_| Msg::Reset)>{ "Reset Config" }</button>
                    // <pre class="wrap" id="log">{"log:"}
                    // { log }</pre>
                </div>
            </div>
        }
    }
}

impl Model {
    fn process(&self){
        if let Some(n) = self.node_copy() {
            let log = self.logger.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let mut node = n.lock().unwrap();
                if let Err(e) = node.process().await{
                    log.info(&format!("Error: {}", e));
                }
            });
        }
    }
    fn node_start(logger: Box<dyn Logger>, link: &ComponentLink<Model>) {
        wasm_bindgen_futures::spawn_local(wrap(
            async {
                let rtc_spawner = Box::new(|cs| WebRTCConnectionSetupWasm::new(cs));
                let my_storage = Box::new(LocalStorage {});
                let ws = WebSocketWasm::new(URL)?;
                let node = Node::new(my_storage, logger, Box::new(ws), rtc_spawner)?;

                Ok(node)
            },
            link.callback(|n: Result<Node, JsValue>| Msg::Node(n)),
        ));
    }

    fn describe(&self) -> String {
        if let Some(n) = self.node_copy() {
            let node = n.lock().unwrap();
            return format!("{} => {}", node.info.info, node.info.public);
        }
        return "Unknown".into();
    }

    fn set_config(data: &str) {
        if let Err(err) = Node::set_config(Box::new(LocalStorage {}), &data) {
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
                n.lock().unwrap().network.get_list().len().to_string()
            } else {
                "N/A".to_string()
            }
        );
    }

    fn nodes_reachable(&self) -> String {
        if let Some(n) = self.node_copy() {
            let node = n.lock().unwrap();
            if let Ok(pings) = node.get_pings_str() {
                if pings.len() > 0 {
                    return format!("Nodes reachable: {}", pings);
                }
            }
        }
        return "Nodes reachable: N/A".into();
    }

    fn node_copy<'a>(&self) -> Option<Arc<Mutex<Node>>> {
        if let Some(n) = &self.node {
            Some(Arc::clone(&n))
        } else {
            None
        }
    }

    fn node_list(&mut self) {
        if let Some(n) = self.node_copy() {
            let mut node = n.lock().unwrap();
            if let Err(e) = node.list() {
                self.logger.error(&format!("Couldn't get list: {:?}", e));
            }
        }
    }

    fn node_ping(&self) {
        if let Some(n) = self.node_copy() {
            let log = self.logger.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let mut node = n.lock().unwrap();
                let str = Date::new_0().to_iso_string().as_string().unwrap();
                if let Err(e) = node.ping(&str).await {
                    log.error(&format!("Couldn't ping node: {:?}", e));
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
