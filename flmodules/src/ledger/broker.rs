use flarch::{data_storage::DataStorage, platform_async_trait, tasks::spawn_local};
use std::error::Error;
use tokio::sync::watch;

use crate::random_connections::messages::{ModuleMessage, RandomIn, RandomMessage, RandomOut};
use flarch::{
    broker::{Broker, BrokerError, Subsystem, SubsystemHandler},
    nodeids::NodeID,
};

use super::{
    core::{LedgerConfig, LedgerStorage, LedgerStorageSave},
    messages::{LedgerIn, LedgerMessage, LedgerMessages, LedgerOut, MessageNode},
};

const MODULE_NAME: &str = "Ledger";

/// The [Ledger] holds the [Translate] and offers convenience methods
/// to interact with [Translate] and [TemplateMessage].
pub struct Ledger {
    /// Represents the underlying broker.
    pub broker: Broker<LedgerMessage>,
    _our_id: NodeID,
    storage: watch::Receiver<LedgerStorage>,
}

impl Ledger {
    pub async fn start(
        mut ds: Box<dyn DataStorage + Send>,
        our_id: NodeID,
        rc: Broker<RandomMessage>,
        config: LedgerConfig,
    ) -> Result<Self, Box<dyn Error>> {
        let str = ds.get(MODULE_NAME).unwrap_or("".into());
        let storage = LedgerStorageSave::from_str(&str).unwrap_or_default();
        let messages = LedgerMessages::new(storage.clone(), config, our_id)?;
        let mut broker = Translate::start(rc, messages).await?;

        let (tx, storage) = watch::channel(storage);
        let (mut tap, _) = broker.get_tap().await?;
        spawn_local(async move {
            loop {
                if let Some(LedgerMessage::Output(LedgerOut::UpdateStorage(sto))) =
                    tap.recv().await
                {
                    tx.send(sto.clone()).expect("updated storage");
                    if let Ok(val) = sto.to_yaml() {
                        ds.set(MODULE_NAME, &val).expect("updating storage");
                    }
                }
            }
        });
        Ok(Ledger {
            broker,
            _our_id: our_id,
            storage,
        })
    }

    pub fn increase_self(&mut self, _counter: u32) -> Result<(), BrokerError> {
        // self.broker
        //     .emit_msg(LedgerIn::Node(self.our_id, MessageNode::Increase(counter)).into())
        Ok(())
    }

    pub fn get_counter(&self) -> u32 {
        self.storage.borrow().counter
    }
}

/// Translates the messages to/from the RandomMessage and calls `TemplateMessages.processMessages`.
struct Translate {
    messages: LedgerMessages,
}

impl Translate {
    async fn start(
        random: Broker<RandomMessage>,
        messages: LedgerMessages,
    ) -> Result<Broker<LedgerMessage>, Box<dyn Error>> {
        let mut template = Broker::new();

        template
            .add_subsystem(Subsystem::Handler(Box::new(Translate { messages })))
            .await?;
        template
            .link_bi(
                random,
                Box::new(Self::link_rnd_template),
                Box::new(Self::link_template_rnd),
            )
            .await?;
        Ok(template)
    }

    fn link_rnd_template(msg: RandomMessage) -> Option<LedgerMessage> {
        if let RandomMessage::Output(msg_out) = msg {
            match msg_out {
                RandomOut::ListUpdate(list) => Some(LedgerIn::UpdateNodeList(list.into()).into()),
                RandomOut::NodeMessageFromNetwork(id, msg) => {
                    if msg.module == MODULE_NAME {
                        serde_yaml::from_str::<MessageNode>(&msg.msg)
                            .ok()
                            .map(|msg_node| LedgerIn::Node(id, msg_node).into())
                    } else {
                        None
                    }
                }
                _ => None,
            }
        } else {
            None
        }
    }

    fn link_template_rnd(msg: LedgerMessage) -> Option<RandomMessage> {
        if let LedgerMessage::Output(LedgerOut::Node(id, msg_node)) = msg {
            Some(
                RandomIn::NodeMessageToNetwork(
                    id,
                    ModuleMessage {
                        module: MODULE_NAME.into(),
                        msg: serde_yaml::to_string(&msg_node).unwrap(),
                    },
                )
                .into(),
            )
        } else {
            None
        }
    }
}

#[platform_async_trait()]
impl SubsystemHandler<LedgerMessage> for Translate {
    async fn messages(&mut self, msgs: Vec<LedgerMessage>) -> Vec<LedgerMessage> {
        let msgs_in = msgs
            .into_iter()
            .filter_map(|msg| match msg {
                LedgerMessage::Input(msg_in) => Some(msg_in),
                LedgerMessage::Output(_) => None,
            })
            .collect();
        self.messages
            .process_messages(msgs_in)
            .into_iter()
            .map(|o| o.into())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use flarch::start_logging_filter_level;

    use super::*;

    #[tokio::test]
    async fn test_increase() -> Result<(), Box<dyn Error>> {
        start_logging_filter_level(vec![], log::LevelFilter::Info);

        Ok(())
    }
}
