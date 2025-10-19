///! Multi template - broker part.
/// This implements a broker with one input and one output message,
/// and is connected to a [Timer] broker as well as a [Network] broker.
/// On every [TimerMessage::Minute], it resets the counter.
/// The new counter value is sent to all other nodes connected
/// through the [Network] broker.
use anyhow::Result;
use flarch::{add_translator_direct, add_translator_link, broker::Broker, tasks::spawn_local};
use tokio::sync::watch;

use crate::{
    network::broker::BrokerNetwork,
    template::multi::message::{InternIn, InternOut, Message},
    timer::{BrokerTimer, Timer},
};

/// The messages this broker takes as input.
#[derive(Debug, Clone, PartialEq)]
pub enum MultiIn {
    Count,
}

/// The messages this broker outputs.
#[derive(Debug, Clone, PartialEq)]
pub enum MultiOut {
    State(Message),
}

/// For other brokers who want to connect to this broker.
pub type BrokerMulti = Broker<MultiIn, MultiOut>;

/// A structure containing one or more states which are automatically updated.
pub struct Multi {
    /// The new state of the template.
    pub state: watch::Receiver<Message>,
    /// A broker to be used in other brokers.
    pub broker: BrokerMulti,
}

impl Multi {
    /// Returns a [Multi] and starts the broker.
    /// It uses a tap to update the state field of the structure.
    pub async fn state(timer: BrokerTimer, network: BrokerNetwork) -> Result<Multi> {
        let (tx, state) = watch::channel(Message::default());
        let mut state = Multi {
            state,
            broker: Self::broker(timer, network).await?,
        };

        let mut tap = state.broker.get_tap_out().await?.0;
        spawn_local(async move {
            while let Some(msg) = tap.recv().await {
                match msg {
                    MultiOut::State(update) => tx.send(update).expect("Updating sum"),
                }
            }
        });

        Ok(state)
    }

    /// Just returns the broker for the multi template.
    pub async fn broker(timer: BrokerTimer, network: BrokerNetwork) -> Result<BrokerMulti> {
        // The broker to interact with this module only contains the relelvant messages
        // to this module.
        let broker = Broker::new();
        // The internal brocker holds all messages - the ones for this module, but also
        // the timer and the network messages.
        let mut intern = Message::broker().await?;
        // Every incoming message from the module broker must go to the input of the internal
        // broker, this is a _direct_ connection: input -> input, output -> output.
        add_translator_direct!(intern, broker.clone(), InternIn::Multi, InternOut::Multi);
        // The output messages of the network broker are mapped to the input queue of the
        // internal broker. This is a _link_ connection: output -> input for both sides.
        add_translator_link!(intern, network, InternIn::Network, InternOut::Network);
        // The timer broker is a special broker, as it only has output messages.
        Timer::minute(timer, intern, InternIn::Timer).await?;

        Ok(broker)
    }
}

#[cfg(test)]
mod test {
    use flarch::nodeids::NodeID;

    use crate::{
        network::broker::{NetworkIn, NetworkOut},
        nodeconfig::NodeInfo,
        template::multi::message::{ModuleMessage, MODULE_NAME},
        timer::TimerMessage,
    };

    use super::*;

    fn state_counter(counter: usize) -> Message {
        let mut m = Message::default();
        m.counter = counter;
        m
    }

    fn state_counter_mo(counter: usize) -> MultiOut {
        MultiOut::State(state_counter(counter))
    }

    #[tokio::test]
    // Testing simple updates of the counter.
    async fn test_update() -> Result<()> {
        let mut timer = Broker::new();
        let mut broker = Multi::broker(timer.clone(), Broker::new()).await?;
        let mut tap = broker.get_tap_out().await?;

        broker.emit_msg_in(MultiIn::Count)?;
        broker.emit_msg_in(MultiIn::Count)?;
        assert_eq!(Some(state_counter_mo(2)), tap.0.recv().await);
        timer.emit_msg_out(TimerMessage::Second)?;
        assert_eq!(Some(state_counter_mo(0)), tap.0.recv().await);
        Ok(())
    }

    #[tokio::test]
    // Testing that the state gets updated correctly.
    async fn test_state() -> Result<()> {
        let mut timer = Broker::new();
        let mut state = Multi::state(timer.clone(), Broker::new()).await?;
        let mut tap = state.broker.get_tap_out().await?;

        state.broker.emit_msg_in(MultiIn::Count)?;
        assert_eq!(state_counter(0), *state.state.borrow());
        state.broker.emit_msg_in(MultiIn::Count)?;
        tap.0.recv().await;
        assert_eq!(state_counter(2), *state.state.borrow());
        timer.emit_msg_out(TimerMessage::Second)?;
        tap.0.recv().await;
        assert_eq!(state_counter(0), *state.state.borrow());

        Ok(())
    }

    #[tokio::test]
    // Testing that the network messages are correctly sent and interpreted.
    async fn test_network() -> Result<()> {
        // flarch::start_logging_filter_level(vec![], log::LevelFilter::Trace);
        let mut timer = Broker::new();
        let mut net = Broker::new();
        let mut tap_net_out = net.get_tap_out().await?.0;
        let mut tap_net_in = net.get_tap_in().await?.0;
        let mut multi = Multi::broker(timer.clone(), net.clone()).await?;
        let mut tap_multi = multi.get_tap_out().await?.0;

        assert_eq!(true, tap_net_out.try_recv().is_err());
        multi.emit_msg_in(MultiIn::Count)?;
        multi.emit_msg_in(MultiIn::Count)?;
        assert_eq!(state_counter_mo(2), tap_multi.recv().await.unwrap());
        assert_eq!(true, tap_multi.try_recv().is_err());

        timer.emit_msg_out(TimerMessage::Minute)?;
        assert_eq!(
            NetworkIn::WSUpdateListRequest,
            tap_net_in.recv().await.unwrap()
        );
        assert_eq!(state_counter_mo(0), tap_multi.recv().await.unwrap());

        let nodes = vec![
            NodeInfo::new_from_id(NodeID::rnd()),
            NodeInfo::new_from_id(NodeID::rnd()),
        ];
        net.emit_msg_out(NetworkOut::NodeListFromWS(nodes.clone()))?;

        multi.emit_msg_in(MultiIn::Count)?;
        multi.emit_msg_in(MultiIn::Count)?;
        let MultiOut::State(state) = tap_multi.recv().await.unwrap();
        assert_eq!(2, state.nodes.len());
        assert_eq!(2, state.counter);
        let NetworkIn::MessageToNode(id1, msg1) = tap_net_in.recv().await.unwrap() else {
            panic!("Got wrong message")
        };
        let NetworkIn::MessageToNode(id2, msg2) = tap_net_in.recv().await.unwrap() else {
            panic!("Got wrong message")
        };
        assert_eq!(nodes[0].get_id(), id1);
        assert_eq!(nodes[1].get_id(), id2);
        assert_eq!(
            Some(ModuleMessage::Counter(2)),
            msg2.unwrap_yaml(MODULE_NAME)
        );

        net.emit_msg_out(NetworkOut::MessageFromNode(id1.clone(), msg1))?;
        let MultiOut::State(state) = tap_multi.recv().await.unwrap();
        assert_eq!(Some(&2usize), state.other.get(&id1));

        Ok(())
    }
}
