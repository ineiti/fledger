# WebSocket and WebRTC implementation for libc

With the publishing of the [webrtc](https://crates.io/crates/webrtc) library,
it became possible to create a fledger-node for libc-based systems.
Beforehand the fledger-node CLI was run using `node`, which was quite heavy.

This crate implements both the WebSocket and the WebRTC for libc-systems.
It uses the `Broker` system from [flutils](../../shared/flutils/) to hide
the callbacks of the libraries.