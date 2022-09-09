# Ping-Pong Example

A simple example for the flnet crate that shows how it can
be used with a shared code and a short implementation for
libc and wasm:

- [shared](shared/README.md) - contains the shared code between the wasm and libc implementation
- [libc](libc/README.md) - a binary implementation using libc
- [wasm](wasm/README.md) - an implementation for wasm using yew and trunk

To run the example, simply type:

```
make test
```

If rust, rust-wasm, node are installed in the correct versions, it will compile the
libc and wasm implementation, and run them.
They should display some `ping`-`pong` logs.
You can add additional tabs with the [wasm](http://localhost:8080) code to have more than two nodes
sending pings to each other.

To stop, do:

```
<ctrl+c>
make kill
```

## What it's showing

This short example shows how to implement a simple logic and then run it both in libc
and wasm.
The runners themselves should only contain the logic relevant to running the code in
the given environment, but not more.
The main logic is in the common `shared` code.
Communication between the runner and the `shared` code is done using the 
[Broker]()