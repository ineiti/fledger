use flnet::network::{connection_state::CSError, node_connection::NCError};
use flutils::broker::BrokerError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TestError {
    #[error(transparent)]
    Broker(#[from] BrokerError),
    #[error(transparent)]
    CS(#[from] CSError),
    #[error(transparent)]
    NC(#[from] NCError),
}

// These two tests are not needed for github-workflow to pass
// mod test_nodes;
// mod test_wait_ms;

mod test_connection_state;
mod test_node_connection;
mod test_webrtc;
