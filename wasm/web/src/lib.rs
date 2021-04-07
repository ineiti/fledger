#![recursion_limit = "1024"]

use js_sys::Date;
use log::{error, info};
use regex::Regex;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use wasm_bindgen::prelude::*;
use yew::{prelude::*, services::IntervalService};

use common::node::{logic::Stat, version::VERSION_STRING};
use wasm_lib::{storage::LocalStorage, web_rtc_setup::WebRTCConnectionSetupWasm, web_socket::WebSocketWasm};
use web_sys::window;

use common::node::Node;

#[cfg(not(feature = "local"))]
const URL: &str = "wss://signal.fledg.re";

#[cfg(feature = "local")]
const URL: &str = "ws://localhost:8765";

struct Model {
    link: ComponentLink<Self>,
    node: Option<Arc<Mutex<Node>>>,
    counter: u32,
    show_reset: bool,
}

enum Msg {
    Tick,
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

        wasm_logger::init(wasm_logger::Config::default());

        Model::node_start(&link);
        let _ = Box::leak(Box::new(IntervalService::spawn(
            Duration::from_secs(1),
            link.callback(|_| Msg::Tick),
        )));
        Self {
            link,
            node: None,
            counter: 0,
            show_reset: false,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Tick => {
                self.counter += 1;
                if self.counter % 5 == 0 || self.counter < 5 {
                    self.node_list();
                    self.node_ping();
                }
                self.process();
            }
            Msg::Node(res_node) => match res_node {
                Ok(node) => {
                    info!("Got node");
                    self.node = Some(Arc::new(Mutex::new(node)));
                }
                Err(e) => {
                    error!("Couldn't create node: {}", e.as_string().unwrap());
                    self.show_reset = true;
                }
            },
            Msg::Reset => {
                Model::set_config("");
                self.show_reset = false;
                Model::node_start(&self.link);
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
                <h2><i>{self.get_info()}</i></h2>
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
                {self.nodes_reachable()}
                <br/>
                <div style="float: right;">{self.get_version()}</div>
                <button style={reset_style} onclick=self.link.callback(|_| Msg::Reset)>{ "Reset Config" }</button>
            </div>
        }
    }
}

impl Model {
    fn process(&self) {
        if let Some(n) = self.node_copy() {
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(mut node) = n.try_lock() {
                    if let Err(e) = node.process().await {
                        info!("Error: {}", e);
                    }
                } else {
                    error!("process couldn't get lock");
                }
            });
        }
    }
    fn node_start(link: &ComponentLink<Model>) {
        wasm_bindgen_futures::spawn_local(wrap(
            async {
                let rtc_spawner = Box::new(|cs| WebRTCConnectionSetupWasm::new(cs));
                let my_storage = Box::new(LocalStorage {});
                let ws = WebSocketWasm::new(URL)?;
                let mut node = Node::new(my_storage, Box::new(ws), rtc_spawner)?;
                if let Some(window) = web_sys::window() {
                    let navigator = window.navigator();
                    node.set_client(match navigator.user_agent() {
                        Ok(p) => p,
                        Err(_) => "n/a".to_string(),
                    });
                }
                node.save()?;

                Ok(node)
            },
            link.callback(|n: Result<Node, JsValue>| Msg::Node(n)),
        ));
    }

    fn get_info(&self) -> String {
        if let Some(n) = self.node_copy() {
            if let Ok(node) = n.try_lock() {
                return node.info().info;
            } else {
                error!("get_info couldn't get lock");
            }
        }
        return "Unknown".into();
    }

    fn set_config(data: &str) {
        if let Err(err) = Node::set_config(Box::new(LocalStorage {}), &data) {
            info!("Got error while saving config: {}", err);
        }
    }

    fn set_localstorage() {
        if let Ok(loc) = window().unwrap().location().href() {
            info!("Location is: {}", loc.clone());
            if loc.contains("#") {
                let reg = Regex::new(r".*?#").unwrap();
                let data_enc = reg.replace(&loc, "");
                if data_enc != "" {
                    info!("Setting data");
                    if let Ok(data) = urlencoding::decode(&data_enc) {
                        Model::set_config(&data);
                    }
                }
            }
        }
    }

    fn get_version(&self) -> String {
        VERSION_STRING.to_string()
    }

    fn nodes_reachable(&self) -> Html {
        let mut out = vec![];
        if let Some(n) = self.node_copy() {
            if let Ok(node) = n.try_lock() {
                let mut stats: Vec<Stat> =
                    node.logic.stats.iter().map(|(_k, v)| v.clone()).collect();
                stats.sort_by(|a, b| b.last_contact.partial_cmp(&a.last_contact).unwrap());
                let now = Date::now();
                for stat in stats {
                    if let Some(ni) = stat.node_info.as_ref() {
                        if node.info().id != ni.id {
                            out.push(vec![
                                format!("{}", ni.info),
                                format!("rx:{} tx:{}", stat.ping_rx, stat.ping_tx),
                                format!("{}s", ((now - stat.last_contact) / 1000.).floor()),
                                format!("in:{:?} out:{:?}", stat.incoming, stat.outgoing),
                            ]);
                        }
                    }
                }
            } else {
                error!("nodes_reachable couldn't get lock");
            }
        }
        if out.len() == 0 {
            return html! {<div>{"Fetching node list"}</div>};
        }
        return html! {
        <table class={"styled-table"}>
            <thead>
                <tr>
                    <th>{"Name"}</th>
                    <th>{"Ping Count"}</th>
                    <th>{"Last ping"}</th>
                    <th>{"Conn. Type"}</th>
                </tr>
            </thead>
            <tbody>
                {out.iter().map(|li|
                    html!{
                        <tr>
                            <td>{li[0].clone()}</td>
                            <td>{li[1].clone()}</td>
                            <td>{li[2].clone()}</td>
                            <td>{li[3].clone()}</td>
                        </tr>
                    }).collect::<Html>()}
            </tbody>
        </table>};
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
            if let Ok(mut node) = n.try_lock() {
                if let Err(e) = node.list() {
                    error!("Couldn't get list: {:?}", e);
                }
            } else {
                error!("node_list couldn't get lock");
            }
        }
    }

    fn node_ping(&self) {
        if let Some(n) = self.node_copy() {
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(mut node) = n.try_lock() {
                    let str = Date::new_0().to_iso_string().as_string().unwrap();
                    if let Err(e) = node.ping(&str).await {
                        error!("Couldn't ping node: {:?}", e);
                    }
                } else {
                    error!("node_ping couldn't get lock");
                }
            });
        }
    }
}

#[wasm_bindgen(start)]
pub async fn run_app() {
    console_error_panic_hook::set_once();
    App::<Model>::new().mount_to_body();
    info!("starting app for now!");
}
