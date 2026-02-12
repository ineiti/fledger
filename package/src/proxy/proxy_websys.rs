//! BroadcastChannel-based leader election for coordinating browser tabs/windows.

use js_sys::Function;
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::BroadcastChannel;

/// Unique identifier for a node, used for leader election.
/// Lower NodeID = higher priority for leadership.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabID {
    timestamp_secs: u64,
    random_component: u32,
}

impl PartialOrd for TabID {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TabID {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.timestamp_secs
            .cmp(&other.timestamp_secs)
            .then_with(|| self.random_component.cmp(&other.random_component))
    }
}

impl std::fmt::Display for TabID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.timestamp_secs, self.random_component)
    }
}

impl TabID {
    fn new() -> Self {
        let timestamp_secs = (js_sys::Date::now() / 1000.0) as u64;
        let mut buf = [0u8; 4];
        getrandom::getrandom(&mut buf).expect("getrandom failed");
        let random_component = u32::from_le_bytes(buf);
        Self {
            timestamp_secs,
            random_component,
        }
    }
}

/// Internal protocol messages sent over BroadcastChannel.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum ChannelMessage {
    Alive(TabID),
    Stopped(TabID),
    ToLeader { from: TabID, data: String },
    FromLeader { data: String },
}

/// Events emitted to JavaScript callback.
#[wasm_bindgen]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyEvent {
    Elected,
    FromLeader,
    ToLeader,
    Ping,
    Stopped,
}

/// Internal state shared between closures.
struct ProxyInner {
    node_id: TabID,
    channel: BroadcastChannel,
    known_nodes: RefCell<HashMap<String, u64>>, // nodeID string -> last_ping_time_ms
    is_leader: Cell<bool>,
    callback: RefCell<Option<Function>>,
    ping_interval: RefCell<Option<i32>>,
    cleanup_interval: RefCell<Option<i32>>,
}

impl ProxyInner {
    fn emit_event(&self, event: ProxyEvent, data: Option<&str>) {
        if let Some(ref callback) = *self.callback.borrow() {
            let event_val = JsValue::from(event as u32);
            let data_val = match data {
                Some(s) => JsValue::from_str(s),
                None => JsValue::NULL,
            };
            let _ = callback.call2(&JsValue::NULL, &event_val, &data_val);
        }
    }

    fn evaluate_leadership(&self) {
        let self_id_str = self.node_id.to_string();
        let known = self.known_nodes.borrow();

        // Collect all node IDs including self
        let mut all_ids: Vec<&str> = known.keys().map(|s| s.as_str()).collect();
        all_ids.push(&self_id_str);

        // Parse and find minimum
        let mut min_id: Option<TabID> = Some(self.node_id);
        for id_str in known.keys() {
            if let Some(parsed) = parse_node_id(id_str) {
                if let Some(current_min) = min_id {
                    if parsed < current_min {
                        min_id = Some(parsed);
                    }
                }
            }
        }

        let should_be_leader = min_id == Some(self.node_id);
        let was_leader = self.is_leader.get();

        if should_be_leader && !was_leader {
            self.is_leader.set(true);
            self.emit_event(ProxyEvent::Elected, None);
        } else if !should_be_leader && was_leader {
            self.is_leader.set(false);
        }
    }

    fn send_message(&self, msg: &ChannelMessage) {
        if let Ok(json) = serde_json::to_string(msg) {
            let _ = self.channel.post_message(&JsValue::from_str(&json));
        }
    }

    fn handle_message(&self, msg: ChannelMessage) {
        match msg {
            ChannelMessage::Alive(node_id) => {
                let id_str = node_id.to_string();
                let now = js_sys::Date::now() as u64;
                self.known_nodes.borrow_mut().insert(id_str.clone(), now);
                self.evaluate_leadership();
                self.emit_event(ProxyEvent::Ping, Some(&id_str));
            }
            ChannelMessage::Stopped(node_id) => {
                let id_str = node_id.to_string();
                self.known_nodes.borrow_mut().remove(&id_str);
                self.emit_event(ProxyEvent::Stopped, Some(&id_str));
                self.evaluate_leadership();
            }
            ChannelMessage::ToLeader { from: _, data } => {
                if self.is_leader.get() {
                    self.emit_event(ProxyEvent::ToLeader, Some(&data));
                }
            }
            ChannelMessage::FromLeader { data } => {
                if !self.is_leader.get() {
                    self.emit_event(ProxyEvent::FromLeader, Some(&data));
                }
            }
        }
    }

    fn cleanup_stale_nodes(&self) {
        let now = js_sys::Date::now() as u64;
        let stale_threshold = 2000; // 2 seconds

        let mut stale_ids = Vec::new();
        {
            let known = self.known_nodes.borrow();
            for (id_str, last_ping) in known.iter() {
                if now - last_ping > stale_threshold {
                    stale_ids.push(id_str.clone());
                }
            }
        }

        for id_str in stale_ids {
            self.known_nodes.borrow_mut().remove(&id_str);
            self.emit_event(ProxyEvent::Stopped, Some(&id_str));
        }

        self.evaluate_leadership();
    }
}

// TODO: replace with a TryFrom<&str> on TabID
fn parse_node_id(s: &str) -> Option<TabID> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() == 2 {
        let timestamp_secs = parts[0].parse().ok()?;
        let random_component = parts[1].parse().ok()?;
        Some(TabID {
            timestamp_secs,
            random_component,
        })
    } else {
        None
    }
}

/// Proxy for coordinating leader election across browser tabs using BroadcastChannel.
#[wasm_bindgen]
pub struct Proxy {
    inner: Rc<ProxyInner>,
}

#[wasm_bindgen]
impl Proxy {
    /// Create a new Proxy with the given channel name.
    #[wasm_bindgen(constructor)]
    pub fn new(channel_name: &str) -> Result<Proxy, JsValue> {
        let node_id = TabID::new();
        let channel = BroadcastChannel::new(channel_name)?;

        let inner = Rc::new(ProxyInner {
            node_id,
            channel,
            known_nodes: RefCell::new(HashMap::new()),
            is_leader: Cell::new(false),
            callback: RefCell::new(None),
            ping_interval: RefCell::new(None),
            cleanup_interval: RefCell::new(None),
        });

        // Set up message listener
        let inner_clone = Rc::clone(&inner);
        let onmessage =
            Closure::<dyn Fn(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
                if let Some(data_str) = event.data().as_string() {
                    if let Ok(msg) = serde_json::from_str::<ChannelMessage>(&data_str) {
                        inner_clone.handle_message(msg);
                    }
                }
            });
        inner
            .channel
            .set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();

        // Set up ping timer (1 second interval)
        let inner_clone = Rc::clone(&inner);
        let ping_closure = Closure::<dyn Fn()>::new(move || {
            let msg = ChannelMessage::Alive(inner_clone.node_id);
            inner_clone.send_message(&msg);
        });
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
        let ping_id = window.set_interval_with_callback_and_timeout_and_arguments_0(
            ping_closure.as_ref().unchecked_ref(),
            1000,
        )?;
        *inner.ping_interval.borrow_mut() = Some(ping_id);
        ping_closure.forget();

        // Set up cleanup timer (500ms interval)
        let inner_clone = Rc::clone(&inner);
        let cleanup_closure = Closure::<dyn Fn()>::new(move || {
            inner_clone.cleanup_stale_nodes();
        });
        let cleanup_id = window.set_interval_with_callback_and_timeout_and_arguments_0(
            cleanup_closure.as_ref().unchecked_ref(),
            500,
        )?;
        *inner.cleanup_interval.borrow_mut() = Some(cleanup_id);
        cleanup_closure.forget();

        // Send initial ping to announce presence
        let msg = ChannelMessage::Alive(inner.node_id);
        inner.send_message(&msg);

        // Set up beforeunload handler to stop proxy when tab closes
        let inner_clone = Rc::clone(&inner);
        let beforeunload_closure = Closure::<dyn Fn()>::new(move || {
            let msg = ChannelMessage::Stopped(inner_clone.node_id);
            inner_clone.send_message(&msg);
            inner_clone.channel.close();
        });
        window.add_event_listener_with_callback(
            "beforeunload",
            beforeunload_closure.as_ref().unchecked_ref(),
        )?;
        beforeunload_closure.forget();

        Ok(Proxy { inner })
    }

    /// Register a callback for proxy events.
    /// Callback signature: (event_type: number, data: string | null)
    #[wasm_bindgen]
    pub fn set_callback(&self, callback: Function) {
        *self.inner.callback.borrow_mut() = Some(callback);
    }

    /// Send a message to the leader.
    /// If this node is the leader, the message is delivered directly.
    #[wasm_bindgen]
    pub fn send_to_leader(&self, data: &str) {
        if self.inner.is_leader.get() {
            self.inner.emit_event(ProxyEvent::ToLeader, Some(data));
        } else {
            let msg = ChannelMessage::ToLeader {
                from: self.inner.node_id,
                data: data.to_string(),
            };
            self.inner.send_message(&msg);
        }
    }

    /// Send a message from the leader to all nodes.
    /// Only works if this node is the leader.
    #[wasm_bindgen]
    pub fn send_from_leader(&self, data: &str) {
        if self.inner.is_leader.get() {
            let msg = ChannelMessage::FromLeader {
                data: data.to_string(),
            };
            self.inner.send_message(&msg);
        }
    }

    /// Check if this node is currently the leader.
    #[wasm_bindgen]
    pub fn is_leader(&self) -> bool {
        self.inner.is_leader.get()
    }

    /// Get this node's ID as a string.
    #[wasm_bindgen]
    pub fn node_id(&self) -> String {
        self.inner.node_id.to_string()
    }

    /// Stop the proxy and announce departure.
    #[wasm_bindgen]
    pub fn stop(&self) {
        // Send stopped message
        let msg = ChannelMessage::Stopped(self.inner.node_id);
        self.inner.send_message(&msg);

        // Clear intervals
        if let Some(window) = web_sys::window() {
            if let Some(id) = *self.inner.ping_interval.borrow() {
                window.clear_interval_with_handle(id);
            }
            if let Some(id) = *self.inner.cleanup_interval.borrow() {
                window.clear_interval_with_handle(id);
            }
        }

        // Close channel
        self.inner.channel.close();
    }
}
