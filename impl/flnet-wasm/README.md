# WASM implementation for the Fledger Network

While the [Fledger network](../../shared/flnet) implements the logic, this part implements
the actual communication traits for:

- Websocket
- WebRTC

This crate works for both the browser and node.
It uses the `Broker` from `flutils` to encapsule the callbacks needed for the websocket-client
and the webrtc module.