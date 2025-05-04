use anyhow::Result;
use chrono::{prelude::DateTime, Utc};
use flmodules::{
    dht_router, dht_storage,
    flo::{
        blob::{BlobPath, FloBlobPage},
        flo::FloID,
        realm::RealmID,
    },
    nodeconfig::NodeInfo,
};
use itertools::Itertools;
use js_sys::JsString;
use regex::Regex;
use std::{
    collections::HashMap,
    mem::ManuallyDrop,
    time::{Duration, UNIX_EPOCH},
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use wasm_bindgen::{prelude::{wasm_bindgen, Closure}, JsCast};
use web_sys::{
    window, Document, Event, HtmlButtonElement, HtmlDivElement, HtmlElement, HtmlInputElement,
    HtmlTextAreaElement,
};

use flarch::{
    data_storage::DataStorageLocal,
    nodeids::{NodeID, U256},
};
use flnode::{node::Node, stat::NetStats, version::VERSION_STRING};

#[wasm_bindgen(module = "/src/main.js")]
extern "C" {
    pub fn downloadFile(fileName: JsString, data: JsString);
    pub fn getEditorContent() -> JsString;
    pub fn setEditorContent(data: JsString);
}

#[derive(Debug, Clone)]
pub enum Button {
    SendMsg,
    _DownloadData,
    WebProxy,
    UpdatePage,
    CreatePage,
    ResetPage,
    EditPage(FloID),
    ViewPage(FloID),
}

/// Web interfaces with the html code.
#[derive(Debug)]
pub struct Web {
    document: Document,
    pub tx: UnboundedSender<Button>,
    pub rx: UnboundedReceiver<Button>,
}

#[derive(Debug)]
pub struct DhtPage {
    pub realm: RealmID,
    pub page: FloBlobPage,
    pub path: String,
}

impl From<FloBlobPage> for DhtPage {
    fn from(value: FloBlobPage) -> Self {
        DhtPage {
            realm: value.realm_id(),
            // TODO: this will not work for paths with multiple elements
            path: value.get_path().unwrap_or(&"".to_string()).into(),
            page: value,
        }
    }
}

impl Web {
    pub fn new() -> Result<Self> {
        Web::set_data_storage();

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Button>();
        let window = web_sys::window().expect("no global `window` exists");
        Ok(Self {
            document: window.document().expect("should have a document on window"),
            tx,
            rx,
        })
    }

    pub fn link_btn(&self, btn: Button, id: &str) {
        let tx = self.tx.clone();
        let cb = ManuallyDrop::new(Closure::wrap(Box::new(move |_: Event| {
            tx.send(btn.clone())
                .err()
                .map(|e| log::error!("Couldn't send message: {e:?}"));
        }) as Box<dyn FnMut(_)>));
        self.document.get_element_by_id(id).map_or_else(
            || {
                log::warn!("Couldn't find button with id: {id}");
            },
            |el| {
                el.add_event_listener_with_callback("click", &cb.as_ref().unchecked_ref())
                    .expect("Should be able to add event listener");
            },
        );
    }

    pub fn set_id_inner(&self, id: &str, inner_html: &str) {
        if self
            .document
            .get_element_by_id(id)
            .map(|el| el.set_inner_html(inner_html))
            .is_none()
        {
            log::warn!("Couldn't set inner html for id: {}", id);
        }
    }

    fn _scroll_to_bottom(&self, id: &str) {
        if let Some(messages_element) = self.document.get_element_by_id(id) {
            messages_element
                .dyn_ref::<web_sys::HtmlElement>()
                .map(|el| el.set_scroll_top(el.scroll_height()));
        }
    }

    pub fn get_element<ET: JsCast>(&self, id: &str) -> ET {
        self.document
            .get_element_by_id(id)
            .expect(&format!("Getting element {id}"))
            .dyn_into::<ET>()
            .map_err(|e| log::error!("Couldn't get element {id}: {e:?}"))
            .unwrap()
    }

    fn get_text_area(&self, id: &str) -> HtmlTextAreaElement {
        self.get_element(id)
    }

    pub fn get_div(&self, id: &str) -> HtmlDivElement {
        self.get_element(id)
    }

    pub fn get_el(&self, id: &str) -> HtmlElement {
        self.get_element(id)
    }

    pub fn get_input(&self, id: &str) -> HtmlInputElement {
        self.get_element(id)
    }

    pub fn get_button(&self, id: &str) -> HtmlButtonElement {
        self.get_element(id)
    }

    pub fn unhide(&self, id: &str) {
        self.set_visible(id, true);
    }

    pub fn set_visible(&self, id: &str, visible: bool) {
        if !visible {
            if let Err(e) = self.get_el(id).class_list().add_1("hidden") {
                log::error!("While adding hidden class from {id}: {e:?}");
            }
        } else {
            if let Err(e) = self.get_el(id).class_list().remove_1("hidden") {
                log::error!("While removing hidden class from {id}: {e:?}");
            }
        }
    }

    pub fn udpate_values(&mut self, val: &Values) {
        self.set_id_inner("node_info", &val.get_node_name());
        self.set_id_inner("username_display", &val.get_node_name());
        self.set_id_inner("version", &val.get_version());
        self.set_id_inner("nodes_online", &format!("{}", val.nodes_online));
        self.set_id_inner("nodes_online_random", &format!("{}", val.nodes_online));
        self.set_id_inner("nodes_connected", &format!("{}", val.nodes_connected));
        self.set_id_inner("msgs_system", &format!("{}", val.msgs_system));
        self.set_id_inner("msgs_local", &format!("{}", val.msgs_local));
        // self.set_html_id("dht_stats", &val.get_dht_stats());
        self.set_id_inner("dht_connections", &val.dht_router.active.to_string());
        self.set_id_inner(
            "realms_count",
            &val.dht_storage.realm_stats.len().to_string(),
        );
        self.set_id_inner(
            "dht_storage_local",
            &human_readable_size(val.dht_storage_local()),
        );
        self.set_id_inner(
            "dht_storage_limit",
            &human_readable_size(val.dht_storage_max()),
        );
        self.set_id_inner("connected_stats", &val.connected_stats());
    }

    pub fn set_editable_pages(&mut self, pages: &Vec<FloBlobPage>) {
        let pages_li = pages
            .iter()
            .map(|page| {
                format!(
                    r#"
            <li>
                <span>{}</span>
                <button class="edit-btn" title="Edit Page" onclick="jsi.button_page_edit('{1:x}')"><i class="fas fa-edit"></i></button>
                <button class="view-btn" title="View Page" onclick="jsi.button_page_view('{1:x}')"><i class="fas fa-eye"></i></button>
            </li>
        "#,
                    page.get_path().unwrap_or(&"unknown".to_string()),
                    page.flo_id()
                )
            })
            .join("");
        self.set_id_inner("editable-pages", &pages_li);
    }

    fn set_data_storage() {
        if let Ok(loc) = window().unwrap().location().href() {
            if loc.contains('#') {
                let reg = Regex::new(r".*?#").unwrap();
                let data_enc = reg.replace(&loc, "");
                if data_enc != "" {
                    if let Ok(data) = urlencoding::decode(&data_enc) {
                        if let Err(err) = Node::set_config(DataStorageLocal::new("fledger"), &data)
                        {
                            log::warn!("Got error while saving config: {}", err);
                        }
                    }
                }
            }
        }
    }

    pub fn get_chat_msg(&self) -> String {
        let your_message = self.get_text_area("your_message");
        let msg = your_message.value();
        your_message.set_value("");
        msg
    }
}

#[derive(Debug)]
pub struct Values {
    info: NodeInfo,
    nodes_info: HashMap<U256, NodeInfo>,
    states: NetStats,
    msgs: FledgerMessages,
    dht_storage: dht_storage::messages::Stats,
    dht_router: dht_router::messages::Stats,
    pub msgs_system: usize,
    pub msgs_local: usize,
    pub mana: u32,
    pub nodes_online: usize,
    pub nodes_connected: usize,
}

#[derive(Clone, Debug, Default)]
pub struct UL(Vec<LI>);

#[derive(Clone, Debug)]
pub struct LI(String, Option<UL>);

impl UL {
    pub fn _new(lis: Vec<LI>) -> Self {
        Self(lis)
    }

    pub fn to_string(&self) -> String {
        format!(
            "<ul>{}</ul>",
            self.0
                .clone()
                .into_iter()
                .map(|hl| hl.to_string())
                .collect::<Vec<_>>()
                .join("")
        )
    }

    pub fn push(&mut self, li: LI) {
        self.0.push(li);
    }
}

impl LI {
    pub fn new(str: &str) -> Self {
        Self(str.to_string(), None)
    }
    pub fn to_string(self) -> String {
        format!(
            "<li>{}{}</li>",
            self.0,
            self.1.map(|ul| ul.to_string()).unwrap_or("".to_string())
        )
    }
}

impl Values {
    pub fn new(node: &Node) -> Self {
        let info = node.node_config.info.clone();
        let msgs = node.gossip.as_ref().unwrap().chat_events();
        let nodes_info = node.nodes_info_all().unwrap();
        Self {
            nodes_online: node.nodes_online().unwrap().len(),
            nodes_connected: node.nodes_connected().unwrap().len(),
            msgs_system: msgs.len(),
            msgs_local: msgs.len(),
            mana: 0,
            msgs: FledgerMessages::new(
                info.get_id(),
                msgs,
                &nodes_info.clone().into_values().collect(),
            ),
            nodes_info,
            dht_router: node.dht_router.as_ref().unwrap().stats.borrow().clone(),
            dht_storage: node.dht_storage.as_ref().unwrap().stats.borrow().clone(),
            states: node.stat.as_ref().unwrap().borrow().clone(),
            info,
        }
    }

    pub fn get_node_name(&self) -> String {
        self.info.name.clone()
    }

    pub fn get_version(&self) -> String {
        VERSION_STRING.to_string()
    }

    pub fn get_msgs(&self) -> String {
        self.msgs.get_messages()
    }

    pub fn get_dht_stats(&self) -> String {
        let mut out = UL(vec![LI(
            format!(
                "Other nodes available: {}",
                self.dht_router
                    .bucket_nodes
                    .iter()
                    .map(|b| format!("{}", b.len()))
                    .collect::<Vec<_>>()
                    .join(" - ")
            ),
            None,
        )]);
        for (rid, stats) in &self.dht_storage.realm_stats {
            let buckets = stats
                .distribution
                .iter()
                .map(|s| format!("{s}"))
                .collect::<Vec<_>>()
                .join(" - ");
            out.0.push(LI(
                format!("Realm {rid}:"),
                Some(UL(vec![
                    LI(
                        format!("Memory usage: {} of {}", stats.size, stats.config.max_space),
                        None,
                    ),
                    LI(format!("Number of Flos (blobs): {}", stats.flos), None),
                    LI(format!("Bucket distribution of Flos: {}", buckets), None),
                ])),
            ))
        }
        out.to_string()
    }

    fn dht_storage_local(&self) -> usize {
        self.dht_storage
            .realm_stats
            .iter()
            .map(|s| s.1.size)
            .sum::<usize>()
    }

    fn dht_storage_max(&self) -> usize {
        self.dht_storage
            .realm_stats
            .iter()
            .map(|s| s.1.config.max_space)
            .sum::<u64>() as usize
    }

    fn connected_stats(&self) -> String {
        self.states
            .states
            .iter()
            .map(|s| {
                format!(
                    "{} - {:?}",
                    self.nodes_info
                        .get(s.0)
                        .map(|ni| format!("{}", ni.name))
                        .unwrap_or(format!("{}", s.0)),
                    s.1.s.type_local
                )
            })
            .sorted()
            .collect::<Vec<String>>()
            .join("<br>")
    }
}

#[derive(Clone, Debug)]
pub struct FledgerMessage {
    from: String,
    date: String,
    text: String,
    our_message: bool,
}

#[derive(Clone, Debug)]
pub struct FledgerMessages {
    msgs: Vec<FledgerMessage>,
}

impl FledgerMessages {
    fn new(
        our_id: NodeID,
        mut tm_msgs: Vec<flmodules::gossip_events::core::Event>,
        nodes: &Vec<NodeInfo>,
    ) -> Self {
        tm_msgs.sort_by(|a, b| a.created.partial_cmp(&b.created).unwrap());
        let mut msgs = vec![];
        for msg in tm_msgs {
            let d = UNIX_EPOCH + Duration::from_secs(msg.created as u64 / 1000);
            // Create DateTime from SystemTime
            let datetime = DateTime::<Utc>::from(d);
            // Formats the combined date and time with the specified format string.
            let date = datetime.format("%A, the %d of %B at %H:%M:%S").to_string();

            let node: Vec<&NodeInfo> = nodes.iter().filter(|&ni| ni.get_id() == msg.src).collect();
            let from = if node.len() == 1 {
                node[0].name.clone()
            } else {
                format!("{}", msg.src)
            };
            msgs.push(FledgerMessage {
                our_message: our_id == msg.src,
                from,
                text: msg.msg.clone(),
                date,
            })
        }
        FledgerMessages { msgs }
    }

    pub fn get_messages(&self) -> String {
        if self.msgs.is_empty() {
            return String::from("No messages");
        }
        self.msgs
            .iter()
            .map(|fm| fm.to_string())
            .collect::<Vec<String>>()
            .join("")
    }
}

impl FledgerMessage {
    pub fn to_string(&self) -> String {
        format!(
            r#"{}
                <div class="message-sender">{}</div>
                <div class="message-content">{}</div>
                <div class="message-time">{}</div>
            </div>
            "#,
            if self.our_message {
                format!(r#"<div class="message-item sent">"#)
            } else {
                format!(r#"<div class="message-item received">"#)
            },
            self.from,
            self.text,
            self.date
        )
    }
}

fn human_readable_size(size: usize) -> String {
    if size == 0 {
        return "0 B".to_string();
    }
    let units = ["B", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
    let size = size as f64;
    let i = (size.ln() / 1024_f64.ln()).floor() as i32;
    let size = size / 1024_f64.powi(i);
    format!("{:.2} {}", size, units[i as usize])
}
