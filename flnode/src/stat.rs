use std::{collections::HashMap, sync::mpsc::Receiver};

use flmodules::network::messages::{NetworkOut, NetworkConnectionState, NetworkMessage};
use flarch::{
    broker::{Broker, BrokerError},
    nodeids::U256,
};

/// Collects the statistics of the connections sent by the network broker.
pub struct StatBroker {
    pub states: HashMap<U256, NetworkConnectionState>,
    tap: Receiver<NetworkMessage>,
}

impl StatBroker {
    pub async fn start(mut broker_net: Broker<NetworkMessage>) -> Result<Self, BrokerError> {
        let (tap, _) = broker_net.get_tap_sync().await?;
        Ok(Self {
            states: HashMap::new(),
            tap,
        })
    }

    pub fn update(&mut self) {
        for msg in self.tap.try_iter() {
            if let NetworkMessage::Output(NetworkOut::ConnectionState(state)) = msg {
                self.states.insert(state.id, state);
            }
        }
    }
}
