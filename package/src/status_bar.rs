use flmodules::nodeconfig::NodeInfo;
use wasm_bindgen::prelude::*;
use web_sys::{window, Document, Element, HtmlElement};

const CSS: &str = include_str!("status_bar.css");
const HTML_TEMPLATE: &str = include_str!("status_bar.html");

pub struct StatusBar {
    doc: Document,
    div: Element,
}

impl StatusBar {
    pub fn new(div_id: &str) -> anyhow::Result<StatusBar> {
        let doc = window()
            .ok_or(anyhow::anyhow!("No window found"))?
            .document()
            .ok_or(anyhow::anyhow!("No document found"))?;
        let div = doc
            .get_element_by_id(div_id)
            .ok_or(anyhow::anyhow!("Element with id '{}' not found", div_id))?;

        let sb = StatusBar { doc, div };
        sb.add_styles().map_err(|e| anyhow::anyhow!("{e:?}"))?;
        sb.add_html().map_err(|e| anyhow::anyhow!("{e:?}"))?;

        Ok(sb)
    }

    pub fn update_node_info(&self, ni: &NodeInfo) -> anyhow::Result<()> {
        // Set initial node info
        self.update_field("node-name", ni.name.as_str())?;
        self.update_field("node-id", &ni.get_id().to_string())?;
        Ok(())
    }

    /// Update connection status with color coding
    pub fn update_connection(&self, connected: bool, connected_dht: bool) -> anyhow::Result<()> {
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

        Ok(())
    }

    // fn update_class(&self, )

    /// Update the DHT node list in the expanded section
    pub fn update_node_list(&self, node_ids: &[String]) -> anyhow::Result<()> {
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
    pub fn _update_page_list(&self, pages: &[String]) -> anyhow::Result<()> {
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

    pub fn update_field(&self, field: &str, value: &str) -> anyhow::Result<()> {
        self.get_element_by_id(&format!("danu-{}", field))?
            .set_text_content(Some(value));
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
    pub fn format_bytes(bytes: usize) -> String {
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
