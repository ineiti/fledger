use async_trait::async_trait;
use flarch::data_storage::DataStorage;
use std::error::Error;

use crate::{
    broker::{Broker, BrokerError, Subsystem, SubsystemHandler},
    nodeids::NodeID,
    random_connections::messages::{ModuleMessage, RandomIn, RandomMessage, RandomOut},
};

use super::{
    core::{TemplateConfig, TemplateStorageSave},
    messages::{MessageNode, TemplateIn, TemplateMessage, TemplateMessages, TemplateOut},
};

const MODULE_NAME: &str = "Template";

/// This links the Template module with other modules, so that
/// all messages are correctly translated from one to the other.
/// For this example, it uses the RandomConnections module to communicate
/// with other nodes.
/// 
/// The [TemplateBroker] holds the [Translate] and offers convenience methods
/// to interact with [Translate] and [TemplateMessage].
pub struct TemplateBroker {
    /// Represents the underlying broker.
    pub broker: Broker<TemplateMessage>,
    our_id: NodeID,
    ds: Box<dyn DataStorage>,
}

impl TemplateBroker {
    pub async fn start(
        ds: Box<dyn DataStorage>,
        our_id: NodeID,
        rc: Broker<RandomMessage>,
        cfg: TemplateConfig,
    ) -> Result<Self, Box<dyn Error>> {
        let broker = Translate::start(ds.clone(), our_id.clone(), rc, cfg).await?;
        Ok(TemplateBroker { broker, ds, our_id })
    }

    pub fn increase_self(&mut self, counter: u32) -> Result<(), BrokerError> {
        self.broker
            .emit_msg(TemplateIn::Node(self.our_id, MessageNode::Increase(counter)).into())
    }

    pub fn get_counter(&self) -> u32 {
        TemplateStorageSave::from_str(&self.ds.get(MODULE_NAME).unwrap_or("".into()))
            .unwrap_or_default()
            .counter
    }
}

struct Translate {
    /// This is always updated with the latest view of the TemplateMessages module.
    messages: TemplateMessages,
    ds: Box<dyn DataStorage + Send>,
}

impl Translate {
    async fn start(
        ds: Box<dyn DataStorage + Send>,
        our_id: NodeID,
        random: Broker<RandomMessage>,
        config: TemplateConfig,
    ) -> Result<Broker<TemplateMessage>, Box<dyn Error>> {
        let str = ds.get(MODULE_NAME).unwrap_or("".into());
        let storage = TemplateStorageSave::from_str(&str).unwrap_or_default();
        let mut template = Broker::new();

        template
            .add_subsystem(Subsystem::Handler(Box::new(Translate {
                messages: TemplateMessages::new(storage, config, our_id)?,
                ds,
            })))
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
                RandomOut::ListUpdate(list) => Some(TemplateIn::UpdateNodeList(list.into()).into()),
                RandomOut::NodeMessageFromNetwork((id, msg)) => {
                    if msg.module == MODULE_NAME {
                        serde_yaml::from_str::<MessageNode>(&msg.msg)
                            .ok()
                            .map(|msg_node| TemplateIn::Node(id, msg_node).into())
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

    fn link_template_rnd(msg: TemplateMessage) -> Option<RandomMessage> {
        if let TemplateMessage::Output(TemplateOut::Node(id, msg_node)) = msg {
            Some(
                RandomIn::NodeMessageToNetwork((
                    id,
                    ModuleMessage {
                        module: MODULE_NAME.into(),
                        msg: serde_yaml::to_string(&msg_node).unwrap(),
                    },
                ))
                .into(),
            )
        } else {
            None
        }
    }

    fn handle_input(&mut self, msg_in: TemplateIn) -> Vec<TemplateOut> {
        match self.messages.process_message(msg_in) {
            Ok(ret) => return ret.into_iter().map(|m| m.into()).collect(),
            Err(e) => log::warn!("While processing message: {e:?}"),
        }
        vec![]
    }

    fn handle_output(&mut self, msg_out: &TemplateOut) {
        if let TemplateOut::StorageUpdate(ts) = msg_out {
            if let Ok(yaml) = ts.to_yaml() {
                if let Err(e) = self.ds.set(MODULE_NAME, &yaml) {
                    log::warn!("While setting TemplateTranslation: {e:?}");
                }
            }
        }
    }
}

#[cfg_attr(feature = "nosend", async_trait(?Send))]
#[cfg_attr(not(feature = "nosend"), async_trait)]
impl SubsystemHandler<TemplateMessage> for Translate {
    async fn messages(&mut self, msgs: Vec<TemplateMessage>) -> Vec<TemplateMessage> {
        let mut out = vec![];
        for msg in msgs {
            log::trace!("Got msg: {msg:?}");
            if let TemplateMessage::Input(msg_in) = msg {
                out.extend(self.handle_input(msg_in));
            }
        }
        for msg in out.iter() {
            log::trace!("Outputting: {msg:?}");
            self.handle_output(msg);
        }
        out.into_iter().map(|o| o.into()).collect()
    }
}

#[cfg(test)]
mod tests {
    use flarch::{data_storage::DataStorageTemp, start_logging_filter_level};

    use super::*;

    #[tokio::test]
    async fn test_increase() -> Result<(), Box<dyn Error>> {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let ds = Box::new(DataStorageTemp::new());
        let id0 = NodeID::rnd();
        let id1 = NodeID::rnd();
        let mut rnd = Broker::new();
        let mut tr = TemplateBroker::start(ds, id0, rnd.clone(), TemplateConfig::default()).await?;
        let mut tap = rnd.get_tap().await?;
        assert_eq!(0, tr.get_counter());

        rnd.settle_msg(RandomMessage::Output(RandomOut::ListUpdate(
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
