//! Shows a minimal status bar for projects to simplify showing
//! the current status of danu.

use flarch::{
    broker::{Broker, SubsystemHandler},
    platform_async_trait,
};
use flmodules::flo::blob::{BlobPath, FloBlobPage};
use tokio::sync::watch;
use wasm_bindgen::prelude::*;
use web_sys::{window, Document, Element, HtmlElement};

use crate::proxy::{
    broadcast::TabID,
    state::{State, StateUpdate},
};

const CSS: &str = include_str!("status_bar.css");
const HTML_TEMPLATE: &str = include_str!("status_bar.html");

#[derive(Debug, Clone)]
pub enum StatusBarIn {
    Update(StateUpdate),
}

pub type BrokerStatusBar = Broker<StatusBarIn, ()>;

pub struct StatusBar {
    state: watch::Receiver<State>,
    doc: Document,
    div: Element,
    id: TabID,
}

impl StatusBar {
    pub async fn new(
        id: TabID,
        div_id: &str,
        state: watch::Receiver<State>,
    ) -> anyhow::Result<BrokerStatusBar> {
        let doc = window()
            .ok_or(anyhow::anyhow!("No window found"))?
            .document()
            .ok_or(anyhow::anyhow!("No document found"))?;
        let div = doc
            .get_element_by_id(div_id)
            .ok_or(anyhow::anyhow!("Element with id '{}' not found", div_id))?;

        let mut broker = Broker::new();
        let sb = StatusBar {
            doc,
            div,
            state,
            id,
        };
        sb.add_styles().map_err(|e| anyhow::anyhow!("{e:?}"))?;
        sb.add_html().map_err(|e| anyhow::anyhow!("{e:?}"))?;
        broker.add_handler(Box::new(sb)).await?;

        Ok(broker)
    }

    fn update_all(&mut self) -> anyhow::Result<()> {
        self.update_node_info()?;
        self.update_connection()?;
        self.update_page_list()?;
        self.update_page_list()?;
        self.update_tabs()?;
        Ok(())
    }

    fn update_state(&mut self, up: StateUpdate) -> anyhow::Result<()> {
        match up {
            StateUpdate::ConnectSignal
            | StateUpdate::ConnectedNodes
            | StateUpdate::AvailableNodes
            | StateUpdate::DisconnectNodes => self.update_connection(),
            StateUpdate::RealmAvailable | StateUpdate::ReceivedFlo(_) => self.update_page_list(),
            StateUpdate::SystemRealm | StateUpdate::DHTStorageStats => self.update_page_list(),
            StateUpdate::NewLeader => self.update_all(),
            StateUpdate::TabList => self.update_tabs(),
        }
    }

    fn update_node_info(&self) -> anyhow::Result<()> {
        // Set initial node info
        let ni = &self.state.borrow().node_info;
        self.update_field_text("node-name", ni.name.as_str())?;
        self.update_field_text("node-id", &ni.get_id().to_string())?;
        Ok(())
    }

    /// Update connection status with color coding
    fn update_connection(&self) -> anyhow::Result<()> {
        let connected = self.state.borrow().config.is_some();
        let connected_dht = !self.state.borrow().nodes_connected_dht.is_empty();
        let element = self.get_element_by_id("danu-connection")?;

        // Remove all status classes
        let class_list = element.class_list();
        class_list.remove_1("danu-status-connected").ok();
        class_list.remove_1("danu-status-connecting").ok();
        class_list.remove_1("danu-status-disconnected").ok();

        // Add appropriate class
        let status = if connected && connected_dht {
            class_list
                .add_1("danu-status-connected")
                .map_err(|e| anyhow::anyhow!("{:?}", e))?;
            "Connected"
        } else if connected {
            class_list
                .add_1("danu-status-connecting")
                .map_err(|e| anyhow::anyhow!("{:?}", e))?;
            "Connecting"
        } else {
            class_list
                .add_1("danu-status-disconnected")
                .map_err(|e| anyhow::anyhow!("{:?}", e))?;
            "Disconnected"
        };
        element.set_text_content(Some(status));

        self.update_node_list()
    }

    /// Update the DHT node list in the expanded section
    fn update_node_list(&self) -> anyhow::Result<()> {
        let state = self.state.borrow();
        self.update_field_text(
            "nodes",
            &format!(
                "{}/{}",
                state.nodes_connected_dht.len(),
                state.nodes_online.len()
            ),
        )?;

        let node_infos = state.nodes_online.clone();
        let node_ids = state
            .nodes_connected_dht
            .iter()
            .map(|n| {
                node_infos
                    .iter()
                    .find(|i| i.get_id() == (*n))
                    .map(|i| i.name.clone())
                    .unwrap_or_else(|| format!("{}", n))
            })
            .collect::<Vec<_>>();

        let list_element = self.get_element_by_id("danu-node-list")?;
        if node_ids.is_empty() {
            list_element.set_inner_html("None");
        } else {
            let html = node_ids
                .iter()
                .map(|id| format!("<div class=\"danu-list-item\">{}</div>", id))
                .collect::<Vec<_>>()
                .join("");
            list_element.set_inner_html(&html);
        }

        Ok(())
    }

    /// Update the stored page list in the expanded section
    fn update_page_list(&self) -> anyhow::Result<()> {
        let mut pages = vec![];
        if let Some(rid) = &self.state.borrow().get_system_realm() {
            if let Some(flos) = self.state.borrow().flos.get(&rid.get_id()) {
                for flo in flos {
                    if let Ok(page) = FloBlobPage::try_from(flo.1 .0.clone()) {
                        pages.push(page.get_path().unwrap_or(&format!("unknown")).clone())
                    }
                }
            }
        }
        let list_element = self.get_element_by_id("danu-page-list")?;

        if pages.is_empty() {
            list_element.set_inner_html("None");
        } else {
            let html = pages
                .iter()
                .map(|page| format!("<div class=\"danu-list-item\">{}</div>", page))
                .collect::<Vec<_>>()
                .join("");
            list_element.set_inner_html(&html);
        }

        Ok(())
    }

    fn update_tabs(&self) -> anyhow::Result<()> {
        let state = self.state.borrow();
        log::info!("Update_tabs {:?}", state.is_leader);
        let role = match state.is_leader {
            Some(true) => "Leader",
            Some(false) => "Follower",
            None => "Searching",
        };
        self.update_field_html(
            "tabs-list",
            &state
                .tab_list
                .iter()
                .map(|tab| {
                    format!(
                        "<div class=\"danu-list-item\">{}</div>",
                        if tab == &self.id {
                            format!("<strong>{}</strong>", tab)
                        } else {
                            format!("{tab}")
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join(""),
        )?;
        self.update_field_text("tab-role", role)
    }

    fn update_field_text(&self, field: &str, value: &str) -> anyhow::Result<()> {
        self.get_element_by_id(&format!("danu-{}", field))?
            .set_text_content(Some(value));
        Ok(())
    }

    fn update_field_html(&self, field: &str, value: &str) -> anyhow::Result<()> {
        self.get_element_by_id(&format!("danu-{}", field))?
            .set_inner_html(value);
        Ok(())
    }

    fn add_styles(&self) -> Result<(), JsValue> {
        let head = self.doc.head().ok_or("No head element found")?;
        let style = self.doc.create_element("style")?;
        style.set_id("danu-status-styles");
        style.set_text_content(Some(CSS));
        head.append_child(&style)?;
        Ok(())
    }

    fn add_html(&self) -> Result<(), JsValue> {
        self.div.set_inner_html(HTML_TEMPLATE);

        // Attach click handler for expand/collapse
        let header = self
            .doc
            .get_element_by_id("danu-header")
            .ok_or("Header element not found")?;

        let doc = self.doc.clone();
        let closure = Closure::wrap(Box::new(move || {
            Self::toggle_status(&doc).ok();
        }) as Box<dyn FnMut()>);

        header
            .dyn_ref::<HtmlElement>()
            .ok_or("Header is not an HtmlElement")?
            .set_onclick(Some(closure.as_ref().unchecked_ref()));

        closure.forget(); // Keep the closure alive

        Ok(())
    }

    /// Toggle between collapsed and expanded state
    fn toggle_status(doc: &Document) -> anyhow::Result<(), JsValue> {
        let details = doc
            .get_element_by_id("danu-details")
            .ok_or(format!("Didn't find danu-details"))?;

        let icon = doc
            .get_element_by_id("danu-expand-icon")
            .ok_or(format!("Didn't find danu-expand-icon"))?;

        let class_list = details.class_list();
        let is_visible = class_list.contains("visible");

        if is_visible {
            class_list.remove_1("visible")?;
            icon.class_list().remove_1("expanded")?;
        } else {
            class_list.add_1("visible")?;
            icon.class_list().add_1("expanded")?;
        }

        Ok(())
    }

    /// Format bytes into human-readable format
    fn _format_bytes(bytes: usize) -> String {
        if bytes < 1024 {
            format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{} KB", bytes / 1024)
        } else {
            format!("{} MB", bytes / (1024 * 1024))
        }
    }

    fn get_element_by_id(&self, id: &str) -> anyhow::Result<Element> {
        self.doc
            .get_element_by_id(id)
            .ok_or(anyhow::anyhow!("Element with id {id} not found"))
    }
}

#[platform_async_trait]
impl SubsystemHandler<StatusBarIn, ()> for StatusBar {
    async fn messages(&mut self, msgs: Vec<StatusBarIn>) -> Vec<()> {
        // log::info!("Got messages: {msgs:?}");
        for msg in msgs {
            let StatusBarIn::Update(up) = msg;
            if let Err(e) = self.update_state(up) {
                log::error!("While updating statusBar: {e:?}");
            }
        }
        vec![]
    }
}
