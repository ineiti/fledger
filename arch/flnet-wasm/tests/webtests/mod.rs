use flnet::web_rtc::node_connection::NCError;
use flmodules::broker::BrokerError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TestError {
    #[error(transparent)]
    Broker(#[from] BrokerError),
    #[error(transparent)]
    NC(#[from] NCError),
}

mod test_webrtc;
mod test_websocket;