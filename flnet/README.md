# FLNet - the networking layer for Fledger

The shared code for the networking layer.
It sets up the websocket-communication with the signalling server,
and can create new WebRTC links to other nodes.
There is also a signal-server implementation, but the network modules
themselves have to be passed from either [flnet-libc](../../libc/flnet-libc/) or
[flnet-wasm](../../wasm/flnet-wasm/).