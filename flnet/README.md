# FLNet - the networking layer for Fledger

FLNet is used in fledger to communicate between browsers.
It uses the WebRTC protocol and a signalling-server with websockets.
The outstanding feature of this implementation is that it works
both for WASM and libc with the same interface.
This allows to write the core code once, and then re-use it for both WASM and libc.

## WebRTC in short

WebRTC is an amazing technique that has been invented to let browsers exchange
data without a third party server.
There are many protocols involved in making this happen, and the first use-case
of WebRTC was to share video and audio directly between two browsers.
But WebRTC also allows for a 'data' channel where you can send any data you want.

For WebRTC to work, the following is needed:

- a WebRTC implementation on both ends: WebRTC exists for browsers, node, and since
  the end of 2021 also as a rust-crate
- a signalling server for the initial setup: as most browsers are behind a router,
  it is not possible to connect directly without some help. The signalling server
  is needed to exchange the information for setting up the communication
- a STUN server: for WebRTC to work also behind a NAT, somebody needs to tell the
  browser which address it has. The STUN server does amazing things like
  poking holes through firewalls, but this doesn't always work, this is why you
  need an optional
- a TURN server: if WebRTC doesn't manage to connect two WebRTC endpoints with each
  other, the last resort is using a TURN server. This is the old-school way of connecting
  two endpoints using a server that forwards traffic in both directions

For a more in-depth presentation, including some pitfalls, I liked this article:
[What is WebRTC and how to avoid its 3 deadliest pitfalls](https://www.mindk.com/blog/what-is-webrtc-and-how-to-avoid-its-3-deadliest-pitfalls/)

## How to use it

Here is a short example of how two nodes can communicate with each other. First of all
you need a locally runing signalling server that you can spin up with:

```rust
cd cli; cargo run --bin flsignal -- -vv
```

For more debugging, it is sometimes useful to add `-vvv`, or even `-vvvv`, but then
it gets very verbose!

Here is a small example of how to setup a server that listens to messages from clients.

```rust
async fn server() -> Result<(), BrokerError> {
    // Create a random node-configuration. It uses serde for easy serialization.
    let nc = flnet::config::NodeConfig::new();
    // Connect to the signalling server and wait for connection requests.
    let mut net = flnet::network_start(nc.clone(), "ws://localhost:8765")
        .await
        .expect("Starting network");

    // Print our ID so it can be copied to the server
    log::info!("Our ID is: {:?}", nc.info.get_id());
    loop {
        // Wait for messages and print them to stdout.
        log::info!("Got message: {:?}", net.recv().await);
    }
}

async fn client(server_id: &str) -> Result<(), BrokerError> {
    // Create a random node-configuration. It uses serde for easy serialization.
    let nc = flnet::config::NodeConfig::new();
    // Connect to the signalling server and wait for connection requests.
    let mut net = flnet::network_start(nc.clone(), "ws://localhost:8765")
        .await
        .expect("Starting network");

    // Need to get the client-id from the message in client()
    let server_id = U256::from_str(server_id).expect("get client id");
    log::info!("Server id is {server_id:?}");
    // This sends the message by setting up a connection using the signalling server.
    // The client must already be running and be registered with the signalling server.
    // Using `SendNodeMessage` will set up a connection using the signalling server, but
    // in the best case, the signalling server will not be used anymore afterwards.
    net.send_msg(server_id, "ping".into())?;

    // Wait for the connection to be set up and the message to be sent.
    wait_ms(1000).await;

    Ok(())
}
```

You can find this code and more examples in the examples directory here:
https://github.com/ineiti/fledger/tree/0.7.0/examples

## Features

FLNet has two features to switch between WASM and libc implementation.
Unfortunately it is not detected automatically yet.

The `wasm` feature enables the WASM backend, which only
includes the WebRTC connections and the signalling client:

```rust
flnet = {features = ["wasm"], version = "0.7"}
```

With the `libc` feature, the libc backend is enabled,
which has the WebRTC connections, and both the signalling client
and server part:

```rust
flnet = {features = ["libc"], version = "0.7"}
```

## Cross-platform usage

If you build a software that will run both in WASM and with libc,
one way to do so is to use [trunk](https://trunkrs.dev/), which allows
you to write the wasm part very similar to the libc part.

In fledger, this is accomplished with the following structure:

```
  flbrowser  fledger-cli
        \    /
        shared
          |
        flnet
```

The `shared` code only depends on the `flnet` crate without a given feature.
It is common between the WASM and the libc implementation.
Only the `flbrowser` and `fledger-cli` depend on the `flnet` crate with the
appropriate feature set.

You can find an example in the [ping-pong](https://github.com/ineiti/fledger/tree/0.7.0/examples/ping-pong/) directory.

## libc

With the publishing of the [webrtc](https://crates.io/crates/webrtc) library,
it became possible to create a fledger-node for libc-based systems.
Beforehand the fledger-node CLI was run using `node`, which was quite heavy.

This crate implements both the WebSocket and the WebRTC for libc-systems.
It uses the `Broker` system from [flmodules](../../shared/flmodules/) to hide
the callbacks of the libraries.

## wasm

This crate implements the `WebSocket` and `WebRTC` trait for the wasm
platform.
This implementation can be used for the browser or for node.
It uses the `Broker` from `flarch` to encapsule the callbacks needed for the websocket-client
and the webrtc module.
