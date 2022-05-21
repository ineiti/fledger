# WASM implementation for the Fledger Network

This crate implements the `WebSocket` and `WebRTC` trait for the wasm
platform.
This implementation can be used for the browser or for node.
It uses the `Broker` from `flarch` to encapsule the callbacks needed for the websocket-client
and the webrtc module.