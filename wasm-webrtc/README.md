# WASM implementation for the Fledger Node

While the [Fledger node](../../common) implements the logic, this part implements
the actual communication traits:

- Websocket
- WebRTC

The goal is to have a portable common code-base that can be used both in a
standard rust CLI, and for a WASM target.
