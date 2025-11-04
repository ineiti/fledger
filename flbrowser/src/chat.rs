use std::{
    collections::HashMap,
    fmt::Display,
    time::{Duration, UNIX_EPOCH},
};

use chrono::{DateTime, Utc};
use flarch::{nodeids::NodeID, tasks::spawn_local_nosend};
use flmodules::{
    gossip_events::{broker::Gossip, broker::GossipOut, core::Category},
    nodeconfig::NodeInfo,
};
use tokio::sync::broadcast;
use web_sys::HtmlDivElement;

use crate::web::{Button, Web};

pub struct Chat {
    gossip: Gossip,
    web: Web,
    msgs: HtmlDivElement,
}

impl Chat {
    pub async fn new(
        mut gossip: Gossip,
        mut rx: broadcast::Receiver<Button>,
    ) -> anyhow::Result<()> {
        let mut tap = gossip.broker.get_tap_out().await?.0;
        let web = Web::new()?;
        let mut c = Chat {
            gossip,
            msgs: web.get_div("messages"),
            web,
        };
        c.show_msgs();
        spawn_local_nosend(async move {
            loop {
                tokio::select! {
                    Ok(btn) = rx.recv() => c.clicked(btn).await,
                    Some(msg) = tap.recv() => c.gossip_msg(msg).await,
                }
            }
        });

        Ok(())
    }

    async fn clicked(&mut self, btn: Button) {
        match btn {
            Button::SendMsg => {
                // User clicked on the `send_msg` button
                let msg = self.web.get_chat_msg();
                if msg != "" {
                    self.gossip
                        .add_chat_message(msg)
                        .await
                        .expect("Should add chat message");
                }
            }
            _ => {}
        }
    }

    async fn gossip_msg(&mut self, msg: GossipOut) {
        match msg {
            GossipOut::NewEvent(e) => {
                if e.category == Category::TextMessage {
                    self.show_msgs();
                }
            }
        }
    }

    fn show_msgs(&mut self) {
        let mut tm_msgs = self.gossip.events(Category::TextMessage);
        let nodes = self
            .gossip
            .events(Category::NodeInfo)
            .iter()
            .filter_map(|ne| NodeInfo::decode(&ne.msg).ok().map(|ni| (ni.get_id(), ni)))
            .collect::<HashMap<NodeID, NodeInfo>>();
        let our_id = self.gossip.info.get_id();
        tm_msgs.sort_by(|a, b| a.created.partial_cmp(&b.created).unwrap());
        let mut msgs = vec![];
        for msg in tm_msgs {
            let d = UNIX_EPOCH + Duration::from_secs(msg.created as u64 / 1000);
            // Create DateTime from SystemTime
            let datetime = DateTime::<Utc>::from(d);
            // Formats the combined date and time with the specified format string.
            let date = datetime.format("%A, the %d of %B at %H:%M:%S").to_string();

            let from = nodes
                .get(&msg.src)
                .map(|ni| ni.name.clone())
                .unwrap_or(msg.src.to_string());
            msgs.push(FledgerMessage {
                our_message: our_id == msg.src,
                from,
                text: msg.msg.clone(),
                date,
            })
        }

        let msgs_str = if msgs.is_empty() {
            String::from("No messages")
        } else {
            msgs.iter()
                .map(|fm| fm.to_string())
                .collect::<Vec<String>>()
                .join("")
        };

        self.web.set_id_inner("messages", &msgs_str);

        self.msgs.set_scroll_top(self.msgs.scroll_height());
    }
}

#[derive(Clone, Debug)]
pub struct FledgerMessage {
    from: String,
    date: String,
    text: String,
    our_message: bool,
}

impl Display for FledgerMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            r#"{}
                <div class="message-sender">{}</div>
                <div class="message-content">{}</div>
                <div class="message-time">{}</div>
            </div>
            "#,
            if self.our_message {
                r#"<div class="message-item sent">"#
            } else {
                r#"<div class="message-item received">"#
            },
            self.from,
            self.text,
            self.date
        )
    }
}
