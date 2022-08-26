# Ping-Pong Example

A simple example for the flnet crate that shows how it can
be used with a shared code and a short implementation for
libc and wasm:

- [shared](shared/src/lib.rs) - contains the shared code between the wasm and libc implementation
- [libc](libc) - a binary implementation
- [wasm](wasm) - an implementation for wasm

To run the example, simply type:

```
make test
```

If rust, rust-wasm, node are installed in the correct versions, it will compile the
libc and wasm implementation, and run them.
They should display some `ping`-`pong` logs.