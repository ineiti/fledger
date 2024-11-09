use flarch::{data_storage::DataStorage, platform_async_trait, tasks::spawn_local};
use std::error::Error;
use tokio::sync::watch;

use crate::{
    overlay::messages::NetworkWrapper,
    random_connections::messages::{RandomIn, RandomMessage, RandomOut},
};
use flarch::{
    broker::{Broker, BrokerError, Subsystem, SubsystemHandler},
    nodeids::NodeID,
};

use super::{
    core::{TemplateConfig, TemplateStorage, TemplateStorageSave},
    messages::{ModuleMessage, TemplateIn, TemplateMessage, TemplateMessages, TemplateOut},
};

const MODULE_NAME: &str = "Template";

/// This links the Template module with other modules, so that
/// all messages are correctly translated from one to the other.
/// For this example, it uses the RandomConnections module to communicate
/// with other nodes.
///
/// The [Template] holds the [Translate] and offers convenience methods
/// to interact with [Translate] and [TemplateMessage].
pub struct Template {
    /// Represents the underlying broker.
    pub broker: Broker<TemplateMessage>,
    our_id: NodeID,
    storage: watch::Receiver<TemplateStorage>,
}

impl Template {
    pub async fn start(
        mut ds: Box<dyn DataStorage + Send>,
        our_id: NodeID,
        rc: Broker<RandomMessage>,
        config: TemplateConfig,
    ) -> Result<Self, Box<dyn Error>> {
        let str = ds.get(MODULE_NAME).unwrap_or("".into());
        let storage = TemplateStorageSave::from_str(&str).unwrap_or_default();
        let messages = TemplateMessages::new(storage.clone(), config, our_id)?;
        let mut broker = Translate::start(rc, messages).await?;

        let (tx, storage) = watch::channel(storage);
        let (mut tap, _) = broker.get_tap().await?;
        spawn_local(async move {
            loop {
                if let Some(TemplateMessage::Output(TemplateOut::UpdateStorage(sto))) =
                    tap.recv().await
                {
                    tx.send(sto.clone()).expect("updated storage");
                    if let Ok(val) = sto.to_yaml() {
                        ds.set(MODULE_NAME, &val).expect("updating storage");
                    }
                }
            }
        });
        Ok(Template {
            broker,
            our_id,
            storage,
        })
    }

    pub fn increase_self(&mut self, counter: u32) -> Result<(), BrokerError> {
        self.broker
            .emit_msg(TemplateIn::FromNetwork(self.our_id, ModuleMessage::Increase(counter)).into())
    }

    pub fn get_counter(&self) -> u32 {
        self.storage.borrow().counter
    }
}

/// Translates the messages to/from the RandomMessage and calls `TemplateMessages.processMessages`.
struct Translate {
    messages: TemplateMessages,
}

impl Translate {
    async fn start(
        random: Broker<RandomMessage>,
        messages: TemplateMessages,
    ) -> Result<Broker<TemplateMessage>, Box<dyn Error>> {
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

    fn link_rnd_template(msg: RandomMessage) -> Option<TemplateMessage> {
        if let RandomMessage::Output(msg_out) = msg {
            match msg_out {
                RandomOut::NodeIDsConnected(list) => {
                    Some(TemplateIn::UpdateNodeList(list.into()).into())
                }
                RandomOut::NetworkWrapperFromNetwork(id, msg) => msg
                    .unwrap_yaml(MODULE_NAME)
                    .map(|msg| TemplateIn::FromNetwork(id, msg).into()),
                _ => None,
            }
        } else {
            None
        }
    }

    fn link_template_rnd(msg: TemplateMessage) -> Option<RandomMessage> {
        if let TemplateMessage::Output(TemplateOut::ToNetwork(id, msg_node)) = msg {
            Some(
                RandomIn::NetworkWrapperToNetwork(
                    id,
                    NetworkWrapper::wrap_yaml(MODULE_NAME, &msg_node).unwrap(),
                )
                .into(),
            )
        } else {
            None
        }
    }
}

#[platform_async_trait()]
impl SubsystemHandler<TemplateMessage> for Translate {
    async fn messages(&mut self, msgs: Vec<TemplateMessage>) -> Vec<TemplateMessage> {
        let msgs_in = msgs
            .into_iter()
            .filter_map(|msg| match msg {
                TemplateMessage::Input(msg_in) => Some(msg_in),
                TemplateMessage::Output(_) => None,
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
    use flarch::{data_storage::DataStorageTemp, start_logging_filter_level};

    use super::*;

    #[tokio::test]
    async fn test_increase() -> Result<(), Box<dyn Error>> {
        start_logging_filter_level(vec![], log::LevelFilter::Info);

        let ds = Box::new(DataStorageTemp::new());
        let id0 = NodeID::rnd();
        let id1 = NodeID::rnd();
        let mut rnd = Broker::new();
        let mut tr = Template::start(ds, id0, rnd.clone(), TemplateConfig::default()).await?;
        let mut tap = rnd.get_tap().await?;
        assert_eq!(0, tr.get_counter());

        rnd.settle_msg(RandomMessage::Output(RandomOut::NodeIDsConnected(
            vec![id1].into(),
        )))
        .await?;
        tr.increase_self(1)?;
        assert!(matches!(
            tap.0.recv().await.unwrap(),
            RandomMessage::Input(_)
        ));
        assert_eq!(1, tr.get_counter());
        Ok(())
    }
}
