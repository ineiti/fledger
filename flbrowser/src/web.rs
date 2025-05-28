use anyhow::Result;
use flmodules::{
    dht_router, dht_storage,
    flo::{
        blob::{BlobPath, FloBlobPage},
        flo::FloID,
    },
    nodeconfig::NodeInfo,
};
use itertools::Itertools;
use js_sys::JsString;
use regex::Regex;
use std::collections::HashMap;
use wasm_bindgen::{prelude::wasm_bindgen, JsCast};
use web_sys::{
    window, Document, HtmlButtonElement, HtmlDivElement, HtmlElement, HtmlInputElement,
    HtmlTextAreaElement,
};

use flarch::{data_storage::DataStorageLocal, nodeids::U256};
use flnode::{node::Node, stat::NetStats, version::VERSION_STRING};

#[wasm_bindgen(module = "/src/main.js")]
extern "C" {
    pub fn downloadFile(fileName: JsString, data: JsString);
    pub fn getEditorContent() -> JsString;
    pub fn setEditorContent(data: JsString);
    pub fn embedPage(data: JsString);
}

#[derive(Debug, Clone)]
pub enum Button {
    SendMsg,
    _DownloadData,
    ProxyRequest,
    UpdatePage,
    CreatePage,
    ResetPage,
    EditPage(FloID),
    ViewPage(FloID),
    DebugPage(FloID),
    AnchorPage(String),
}

#[derive(PartialEq)]
pub enum Tab {
    Home,
    Page,
    Chat,
    Proxy,
}

impl TryFrom<String> for Button {
    type Error = anyhow::Error;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        match value {
            _ if value == "send-message" => Ok(Button::SendMsg),
            _ if value == "proxy-request" => Ok(Button::ProxyRequest),
            _ if value == "create-page" => Ok(Button::CreatePage),
            _ if value == "update-page" => Ok(Button::UpdatePage),
            _ if value == "reset-page" => Ok(Button::ResetPage),
            _ => Err(anyhow::anyhow!("Don't know {value}")),
        }
    }
}

/// Web interfaces with the html code.
#[derive(Debug)]
pub struct Web {
    document: Document,
}

impl Web {
    pub fn new() -> Result<Self> {
        Web::set_data_storage();

        let window = web_sys::window().expect("no global `window` exists");
        Ok(Self {
            document: window.document().expect("should have a document on window"),
        })
    }

    pub fn get_hash(&self) -> Option<String> {
        self.document.location().and_then(|l| l.hash().ok())
    }

    pub fn set_hash(&mut self, path: &str) {
        self.document.location().map(|l| l.set_hash(path));
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

    pub fn set_editable_pages(&mut self, pages: &Vec<(String, FloID)>) {
        let pages_li = pages
            .iter()
            .map(|(path, id)| {
                format!(
                    r#"
            <li>
                <span>{}</span>
                <button class="edit-btn" title="Debug Page" onclick="jsi.button_page_debug('{1:x}')"><i class="fas fa-cog"></i></button>
                <button class="edit-btn" title="Edit Page" onclick="jsi.button_page_edit('{1:x}')"><i class="fas fa-edit"></i></button>
                <button class="view-btn" title="View Page" onclick="jsi.button_page_view('{1:x}')"><i class="fas fa-eye"></i></button>
            </li>
        "#,
                    path,
                    id
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

    pub fn set_tab(&mut self, tab: Tab) {
        for t in Tab::all() {
            let id = t.to_id();
            let class = (t == tab).then_some("active").unwrap_or("");
            self.get_el(&format!("{id}-module"))
                .set_class_name(&format!("{class} module-content"));
            self.get_el(&format!("menu-{id}")).set_class_name(class);
        }
    }

    pub fn page_family(&mut self, parents: &[&FloBlobPage], children: &[&FloBlobPage]) {
        if self.hide_empty("page_family", parents.is_empty() && children.is_empty()) {
            return;
        }
        self.page_fill_list("page_family_parents", parents);
        self.page_fill_list("page_family_children", children);
    }

    pub fn page_cuckoos(&mut self, parent: Option<&FloBlobPage>, attached: &[&FloBlobPage]) {
        if self.hide_empty("page_cuckoos", parent.is_none() && attached.is_empty()) {
            return;
        }
        self.page_fill_list(
            "page_cuckoos_parent",
            &parent.as_ref().map(|&p| vec![p]).unwrap_or(vec![]),
        );
        self.page_fill_list("page_cuckoos_attached", attached);
    }

    fn hide_empty(&mut self, id: &str, empty: bool) -> bool {
        self.get_el(id)
            .set_class_name(empty.then(|| "hidden").unwrap_or(""));
        empty
    }

    fn page_fill_list(&mut self, id: &str, pages: &[&FloBlobPage]) {
        if !self.hide_empty(id, pages.is_empty()) {
            let pages_ul = pages
                .iter()
                .map(|c| LI::new(&Self::get_page_link(c)))
                .collect::<Vec<_>>();
            self.set_id_inner(&format!("{id}_list"), &UL::new(pages_ul).to_string());
        }
    }

    fn get_page_link(dp: &FloBlobPage) -> String {
        let path = dp
            .get_path()
            .unwrap_or(&format!("{}", dp.flo_id()))
            .to_string();
        format!("<a href='#web/{path}'>{path}</a>")
    }
}

impl Tab {
    fn to_id(&self) -> String {
        match self {
            Tab::Home => "home",
            Tab::Page => "page-edit",
            Tab::Chat => "chat",
            Tab::Proxy => "proxy",
        }
        .to_string()
    }

    fn all() -> Vec<Tab> {
        vec![Self::Home, Self::Page, Self::Chat, Self::Proxy]
    }
}

#[derive(Debug)]
pub struct Values {
    info: NodeInfo,
    nodes_info: HashMap<U256, NodeInfo>,
    states: NetStats,
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
    pub fn new(lis: Vec<LI>) -> Self {
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
            .map(|s| s.1.size as usize)
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
