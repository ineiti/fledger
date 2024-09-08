<p align="center">
  WebRTC data communication for libc and wasm
</p>

<p align="center">
  <a href="https://github.com/ineiti/fledger/actions?query=branch%3Amain">
    <img alt="Build Status" src="https://img.shields.io/github/workflow/status/ineiti/fledger/CI/main">
  </a>

  <a href="https://crates.io/crates/flnet">
    <img alt="Crates.io" src="https://img.shields.io/crates/v/flnet.svg">
  </a>
</p>

<p align="center">
  <a href="https://docs.rs/flnet/">
    Documentation
  </a> | <a href="https://web.fledg.re/">
    Website
  </a>
</p>

FLNet is used in [fledger](https://web.fledg.re) to communicate between browsers.
It uses the WebRTC protocol and a signalling-server with websockets.
The outstanding feature of this implementation is that it works
both for WASM and libc with the same interface.
This allows to write the core code once, and then re-use it for both WASM and libc.

# How to use it

To include flnet in your code, add the following line to your `Cargo.toml` file:

```toml
# For the common code
flnet = "0.7"
# For the wasm implementation
flnet = {features = ["wasm"], version = "0.7"}
# For the libc implementation
flnet = {features = ["libc"], version = "0.7"}
```

The `libc` feature also offers a signalling-server and a websocket-server.

# License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.