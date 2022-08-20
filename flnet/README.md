# FLNet - the networking layer for Fledger

FLNet is used in fledger to communicate between browsers.
It uses the WebRTC protocol and a signalling-server with websockets.
The outstanding feature of this implementation is that it works
both for WASM and libc with the same interface.
This allows to write the core code once, and then re-use it for both WASM and libc.

## Features

FLNet has two features to switch between WASM and libc implementation.
Unfortunately it is not detected automatically yet.

The `wasm` feature enables the WASM backend, which only
includes the WebRTC connections and the signalling client:

```
flnet = {features = ["wasm"], version = "0.6"}
```

With the `libc` feature, the libc backend is enabled,
which has the WebRTC connections, and both the signalling client
and server part:

```
flnet = {features = ["libc"], version = "0.6"}
```

## Cross-platform usage

If you build a software that will run both in WASM and with libc,
you will have to take care of how they will be called:
- WASM needs an implementation as a library
- libc will want an implementation as a binary

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

You can find an example in the [ping-pong](examples/ping-pong/) directory.

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