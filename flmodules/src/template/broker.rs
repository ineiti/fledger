use flarch::data_storage::DataStorage;
use tokio::sync::watch;

use crate::{
    random_connections::broker::{BrokerRandom, RandomIn, RandomOut},
    router::messages::NetworkWrapper,
};
use flarch::{broker::Broker, nodeids::NodeID};

use super::{
    core::{TemplateConfig, TemplateStorage},
    messages::{Messages, ModuleMessage, TemplateIn, TemplateOut},
};

pub type BrokerEndpoint = Broker<TemplateIn, TemplateOut>;

pub(super) const MODULE_NAME: &str = "Template";

/// This links the Template module with other modules, so that
/// all messages are correctly translated from one to the other.
/// For this example, it uses the RandomConnections module to communicate
/// with other nodes.
///
/// The [Template] holds the [Translate] and offers convenience methods
/// to interact with [Translate] and [TemplateMessage].
#[derive(Clone)]
pub struct Template {
    /// Represents the underlying broker.
    pub broker: Broker<TemplateIn, TemplateOut>,
    our_id: NodeID,
    storage: watch::Receiver<TemplateStorage>,
}

impl Template {
    pub async fn start(
        ds: Box<dyn DataStorage + Send>,
        our_id: NodeID,
        rc: BrokerRandom,
        config: TemplateConfig,
    ) -> anyhow::Result<Self> {
        let (messages, storage) = Messages::new(ds, config, our_id);
        let mut broker = Broker::new();
        broker.add_handler(Box::new(messages)).await?;

        broker
            .add_translator_link(
                rc,
                Box::new(Self::link_template_rnd),
                Box::new(Self::link_rnd_template),
            )
            .await?;

        Ok(Template {
            broker,
            our_id,
            storage,
        })
    }

    pub fn increase_self(&mut self, counter: u32) -> anyhow::Result<()> {
        self.broker.emit_msg_in(TemplateIn::FromNetwork(
            self.our_id,
            ModuleMessage::Increase(counter),
        ))
    }

    pub fn get_counter(&self) -> u32 {
        self.storage.borrow().counter
    }

    fn link_rnd_template(msg: RandomOut) -> Option<TemplateIn> {
        match msg {
            RandomOut::NodeIDsConnected(list) => Some(TemplateIn::UpdateNodeList(list.into())),
            RandomOut::NetworkWrapperFromNetwork(id, msg) => msg
                .unwrap_yaml(MODULE_NAME)
                .map(|msg| TemplateIn::FromNetwork(id, msg)),
            _ => None,
        }
    }

    fn link_template_rnd(msg: TemplateOut) -> Option<RandomIn> {
        let TemplateOut::ToNetwork(id, msg_node) = msg;
        Some(RandomIn::NetworkWrapperToNetwork(
            id,
            NetworkWrapper::wrap_yaml(MODULE_NAME, &msg_node).unwrap(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use flarch::{data_storage::DataStorageTemp, start_logging_filter_level};

    use super::*;

    #[tokio::test]
    async fn test_increase() -> anyhow::Result<()> {
        start_logging_filter_level(vec![], log::LevelFilter::Info);

        let ds = Box::new(DataStorageTemp::new());
        let id0 = NodeID::rnd();
        let id1 = NodeID::rnd();
        let mut rnd = Broker::new();
        let mut tr = Template::start(ds, id0, rnd.clone(), TemplateConfig::default()).await?;
        assert_eq!(0, tr.get_counter());

        rnd.settle_msg_out(RandomOut::NodeIDsConnected(vec![id1].into()))
            .await?;
        tr.increase_self(1)?;
        tr.broker.settle(vec![]).await?;
        assert_eq!(1, tr.get_counter());
        Ok(())
    }
}
