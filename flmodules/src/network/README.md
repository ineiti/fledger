The network module is used in [fledger](https://web.fledg.re) to communicate between browsers.
It uses the WebRTC protocol and a signalling-server with websockets.
The outstanding feature of this implementation is that it works
both for WASM and libc with the same interface.
This allows to write the core code once, and then re-use it for both WASM and libc.
