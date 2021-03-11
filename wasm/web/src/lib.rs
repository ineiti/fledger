#![recursion_limit = "1024"]

use common::node::{ext_interface::Logger, logic::Stat};
use std::sync::Arc;
use std::sync::Mutex;
use yew::services::IntervalService;

use std::time::Duration;

use wasm_bindgen::prelude::*;
use yew::prelude::*;

use js_sys::Date;
use regex::Regex;
use wasm_lib::{
    storage_logs::{ConsoleLogger, LocalStorage},
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
    logger: ConsoleLogger,
    counter: u32,
    show_reset: bool,
    no_contact_yet: bool,
}

enum Msg {
    UpdateLog,
    Reset,
    Node(Result<Node, JsValue>),
}

async fn wrap<F: std::future::Future>(f: F, done_cb: yew::Callback<F::Output>) {
    done_cb.emit(f.await);
}

impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        console_error_panic_hook::set_once();
        Model::set_localstorage();

        let logger = ConsoleLogger {};
        Model::node_start(logger.clone(), &link);
        let _ = Box::leak(Box::new(IntervalService::spawn(
            Duration::from_secs(1),
            link.callback(|_| Msg::UpdateLog),
        )));
        Self {
            link,
            node: None,
            counter: 0,
            show_reset: false,
            no_contact_yet: true,
            logger,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::UpdateLog => {
                if let Some(n) = self.node_copy() {
                    if self.no_contact_yet {
                        if let Ok(mut node) = n.try_lock(){
                            let l = node.get_list();
                            self.logger.info(&format!("List length is: {:?}", l));
                            if l.len() > 0 {
                                self.no_contact_yet = false;
                                self.node_ping();
                            }
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
        let reset_style = match self.show_reset {
            false => "display: none;",
            true => "",
        };
        html! {
            <div class="main">
                <h1>{"Fledger Web Node"}</h1>
                <p>{"The goal of Fledger is to have a full node running in the web browser.
                Instead of having to invest in big hardware, fledger will be light enough
                that you can participate using a browser.
                You will mine Mana can be used to run smart contracts, store data, or use the
                re-encryption service."}</p>
                <p>{"The state of the project is"}</p>
                <ul>
                    <li>{"In process: learning Rust and making WebRTC work reliably"}</li>
                    <li>{"Coming up: Set up Identity Chain"}</li>
                    <li>{"Coming up: Add shard chains"}</li>
                    <li>{"Coming up: Other chains: memory and secret"}</li>
                </ul>
                <p>{"For more information, see the documentation: "}
                <a href={"https://fledg.re/doc/index.html"} target={"other"}>{"Fledger - the blockchain that could"}</a>
                </p>
                <div class="ui">
                    <ul>
                        <li>{"Our node: "}{self.describe()}</li>
                        <li>{"Known Nodes:"}{self.nodes_reachable()}</li>
                    </ul>
                    <button style={reset_style} onclick=self.link.callback(|_| Msg::Reset)>{ "Reset Config" }</button>
                </div>
            </div>
        }
    }
}

impl Model {
    fn process(&self) {
        if let Some(n) = self.node_copy() {
            let log = self.logger.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(mut node) = n.try_lock(){
                    if let Err(e) = node.process().await {
                        log.info(&format!("Error: {}", e));
                    }
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
            if let Ok(node) = n.try_lock(){
                return format!("{} => {}", node.info.info, node.info.public);
            }
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

    fn nodes_reachable(&self) -> Html {
        let mut out = vec![];
        if let Some(n) = self.node_copy() {
            if let Ok(node) = n.try_lock(){
                let mut stats: Vec<Stat> = node.logic.stats.iter().map(|(_k, v)| v.clone()).collect();
                stats.sort_by(|a, b| b.last_contact.partial_cmp(&a.last_contact).unwrap());
                let now = Date::now();
                for stat in stats {
                    if let Some(ni) = stat.node_info.as_ref() {
                        if node.info.public != ni.public {
                            out.push(vec![
                                format!("{}", ni.info),
                                format!("{} / {}", stat.ping_rx, stat.ping_tx),
                                format!("{}s", ((now-stat.last_contact) / 1000.).floor()),
                                format!("{:?} / {:?}", stat.incoming, stat.outgoing),
                            ]);
                        }
                    }
                }
            }
        }
        return html! {
        <ul class={"no-bullets"}>
            <li class={"info"}>
                <span class={"info_node"}>{"Name"}</span>
                <span class={"info_node"}>{"Count (rx/tx)"}</span>
                <span class={"info_node"}>{"Last seen"}</span>
                <span class={"info_node"}>{"Conn Stat (in/out)"}</span>
            </li>
        {out.iter().map(|li|
            html!{
                <li class={"info"}>
                    <span class={"info_node"}>{li[0].clone()}</span>
                    <span class={"info_node"}>{li[1].clone()}</span>
                    <span class={"info_node"}>{li[2].clone()}</span>
                    <span class={"info_node"}>{li[3].clone()}</span>
                </li>
            }).collect::<Html>()}
        </ul>};
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
            if let Ok(mut node) = n.try_lock(){
                if let Err(e) = node.list() {
                    self.logger.error(&format!("Couldn't get list: {:?}", e));
                }
            }
        }
    }

    fn node_ping(&self) {
        if let Some(n) = self.node_copy() {
            let log = self.logger.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(mut node) = n.try_lock(){
                    let str = Date::new_0().to_iso_string().as_string().unwrap();
                    if let Err(e) = node.ping(&str).await {
                        log.error(&format!("Couldn't ping node: {:?}", e));
                    }
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
