//! # flmodules::network - sending data over WebRTC in libc and wasm
//!
//! flmodules::network is used in fledger to communicate between browsers.
//! It uses the WebRTC protocol and a signalling-server with websockets.
//! The outstanding feature of this implementation is that it works
//! both for WASM and libc with the same interface.
//! This allows to write the core code once, and then re-use it for both WASM and libc.
//!
//! ## WebRTC in short
//!
//! WebRTC is an amazing technique that has been invented to let browsers exchange
//! data without a third party server.
//! There are many protocols involved in making this happen, and the first use-case
//! of WebRTC was to share video and audio directly between two browsers.
//! But WebRTC also allows for a 'data' channel where you can send any data you want.
//!
//! For WebRTC to work, the following is needed:
//!
//! - a WebRTC implementation on both ends: WebRTC exists for browsers, node, and since
//!   the end of 2021 also as a rust-crate
//! - a [signalling server](https://crates.io/crates/flsignal)
//!   for the initial setup: as most browsers are behind a router,
//!   it is not possible to connect directly without some help. The signalling server
//!   is needed to exchange the information for setting up the communication
//! - a STUN server: for WebRTC to work also behind a NAT, somebody needs to tell the
//!   browser which address it has. The STUN server does amazing things like
//!   poking holes through firewalls, but this doesn't always work, this is why you
//!   need an optional
//! - a TURN server: if WebRTC doesn't manage to connect two WebRTC endpoints with each
//!   other, the last resort is using a TURN server. This is the old-school way of connecting
//!   two endpoints using a server that forwards traffic in both directions
//!
//! Here is a
//! [docker-compose.signalling.yaml](https://github.com/ineiti/fledger/tree/0.7.0/examples/docker-compose/docker-compose.signalling.yaml)
//! ready to run the `flsignal` and [coturn](https://github.com/coturn/coturn).
//!
//! For a more in-depth presentation, including some pitfalls, I liked this article:
//! [What is WebRTC and how to avoid its 3 deadliest pitfalls](https://www.mindk.com/blog/what-is-webrtc-and-how-to-avoid-its-3-deadliest-pitfalls/)
//!
//! ## Example client/server
//!
//! Here is a short example of how two nodes can communicate with each other. First of all
//! you need a locally runing signalling server that you can spin up with:
//!
//! ```bash
//! cd cli; cargo run --bin flsignal -- -v
//! ```
//!
//! For more debugging, it is sometimes useful to add `-vvv`, or even `-vvvv`, but then
//! it gets very verbose!
//!
//! Here is a small example of how to setup a server that listens to messages from clients.
//!
//! ```bash
//! // The server waits for connections by the clients and prints the messages received.
//! async fn server() -> anyhow::Result<()> {
//!     // Create a random node-configuration. It uses serde for easy serialization.
//!     let nc = flmodules::network::config::NodeConfig::new();
//!     // Connect to the signalling server and wait for connection requests.
//!     let mut net = flmodules::network::network_start(nc.clone(), "ws://localhost:8765")
//!         .await
//!         .expect("Starting network");
//!
//!     // Print our ID so it can be copied to the server
//!     log::info!("Our ID is: {:?}", nc.info.get_id());
//!     loop {
//!         // Wait for messages and print them to stdout.
//!         log::info!("Got message: {:?}", net.recv().await);
//!     }
//! }
//! ```
//!
//! It is contacted by the client node, which needs the ID of the server to know where
//! to connect to:
//!
//! ```bash
//! // A client sends a single message to a server and then disconnects.
//! async fn client(server_id: &str) -> anyhow::Result<()> {
//!     // Create a random node-configuration. It uses serde for easy serialization.
//!     let nc = flmodules::network::config::NodeConfig::new();
//!     // Connect to the signalling server and wait for connection requests.
//!     let mut net = flmodules::network::network_start(nc.clone(), "ws://localhost:8765")
//!         .await
//!         .expect("Starting network");
//!
//!     // Need to get the client-id from the message in client()
//!     let server_id = U256::from_str(server_id).expect("get client id");
//!     log::info!("Server id is {server_id:?}");
//!     // This sends the message by setting up a connection using the signalling server.
//!     // The client must already be running and be registered with the signalling server.
//!     // Using `MessageToNode` will set up a connection using the signalling server, but
//!     // in the best case, the signalling server will not be used anymore afterwards.
//!     net.send_msg(server_id, "ping".into())?;
//!
//!     // Wait for the connection to be set up and the message to be sent.
//!     wait_ms(1000).await;
//!
//!     Ok(())
//! }
//! ```
//!
//! You can find this code and more examples in the examples directory here:
//! <https://github.com/ineiti/fledger/tree/0.7.0/examples>
//! Other examples include a full example with a shared code, a libc and a wasm implementation:
//! [Ping-Pong](https://github.com/ineiti/fledger/tree/0.7.0/examples/ping-pong).
//!
//! ## Features
//!
//! flmodules::network has two features to choose between WASM and libc implementation.
//! Unfortunately it is not detected automatically yet.
//! If you write code that is shared between the two, you can add
//! `flmodules::network` without any features to your `Cargo.toml`.
//! The appropriate feature will then be added once you compile the
//! final code.
//!
//! The `wasm` feature enables the WASM backend, which only
//! includes the WebRTC connections and the signalling client:
//!
//! ```cargo.toml
//! flmodules::network = {features = ["wasm"], version = "0.7"}
//! ```
//!
//! With the `libc` feature, the libc backend is enabled,
//! which has the WebRTC connections, and both the signalling client
//! and server part:
//!
//! ```cargo.toml
//! flmodules::network = {features = ["libc"], version = "0.7"}
//! ```
//!
//! ## Cross-platform usage
//!
//! If you build a software that will run both in WASM and with libc,
//! one way to do so is to use [trunk](https://trunkrs.dev/), which allows
//! you to write the wasm part very similar to the libc part.
//!
//! In fledger, this is accomplished with the following structure:
//!
//! ```text
//!   flbrowser  fledger-cli
//!         \    /
//!         shared
//!           |
//!         flmodules::network
//! ```
//!
//! The `shared` code only depends on the `flmodules::network` crate without a given feature.
//! It is common between the WASM and the libc implementation.
//! Only the `flbrowser` and `fledger-cli` depend on the `flmodules::network` crate with the
//! appropriate feature set.
//!
//! You can find an example in the [ping-pong](https://github.com/ineiti/fledger/tree/0.7.0/examples/ping-pong/) directory.
//!
//! ## libc
//!
//! The libc code is made possible by the excellent [webrtc](https://crates.io/crates/webrtc) library.
//! The `libc` feature enables both the WebSocket-server and the WebRTC for libc-systems.
//!
//! ## wasm
//!
//! This feature implements the `WebSocket` and `WebRTC` trait for the wasm
//! platform.
//! It can be used for the browser or for node.

use broker::Network;
use thiserror::Error;

pub mod broker;
pub mod signal;

use flarch::{
    broker::BrokerError,
    web_rtc::{
        connection::ConnectionConfig, web_rtc_setup::web_rtc_spawner,
        web_socket_client::WebSocketClient, websocket::WSClientError, WebRTCConn,
    },
};

use crate::nodeconfig::NodeConfig;

#[derive(Error, Debug)]
/// Errors when setting up a new network connection
pub enum NetworkSetupError {
    /// Something went wrong with the broker initialization
    #[error(transparent)]
    Broker(#[from] BrokerError),
    /// Problem in the websocket client
    #[error(transparent)]
    WebSocketClient(#[from] WSClientError),
    /// Generic network error
    #[error(transparent)]
    Network(#[from] broker::NetworkError),
    #[cfg(target_family = "unix")]
    /// Problem in the websocket server
    #[error(transparent)]
    WebSocketServer(#[from] flarch::web_rtc::websocket::WSSError),
}

/// Starts a new [`BrokerNetwork`] with a given `node`- and `connection`-configuration.
/// This returns a raw broker which is mostly suited to connect to other brokers.
/// If you need an easier access to the WebRTC network, use [`network_webrtc_start`], which returns
/// a structure with a more user-friendly API.
///
/// # Example
///
/// ```bash
/// async fn start_network() -> anyhow::Result<()>{
///   let net = network_broker_start();
/// }
/// ```
pub async fn network_start(
    node: NodeConfig,
    connection: ConnectionConfig,
) -> anyhow::Result<Network> {
    use crate::network::broker::Network;

    let ws = WebSocketClient::connect(&connection.signal()).await?;
    let webrtc = WebRTCConn::new(web_rtc_spawner(connection)).await?;
    Ok(Network::start(node.clone(), ws, webrtc).await?)
}

/// Starts a new connection to the signalling server using the `node`-
/// and `connection`-configuration.
/// It returns a user-friendly API that can be used to send and
/// receive messages.
/// If you want to connect the network with other brokers, then use
/// the [`network_start`] method.
pub async fn network_webrtc_start(
    node: NodeConfig,
    connection: ConnectionConfig,
) -> anyhow::Result<broker::NetworkWebRTC> {
    let net = network_start(node, connection).await?;
    Ok(broker::NetworkWebRTC::start(net.broker).await?)
}
