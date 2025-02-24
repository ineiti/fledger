# FlArch

The Fledger Arch module holds common methods that are used by libc and wasm
implementation.
The following methods / structures are available:
- `DataStorage` allows to store key/value pairs in a file / localStorage
- `tasks::*` various useful tools:
  - `now() -> i64` - returns the current timestamp in milliseconds as i64
  - `spawn_local<F: Future<Output = ()> + 'static>(f: F)` - spawns a future locally
  - `wait_ms(ms: u32)` - async wait in milliseconds
  - `interval(dur: Duration)` - creates a stream that will send the expected time of resolution every `dur`
  - `Interval` - a stream created by `interval`

## Features

- `node` compiles for the node target

# Broker

I wanted to create a common code for both the libc- and wasm-implementation for
Fledger.
Unfortunately it is difficult by the fact that libc allows to use threads
(and sometimes needs them), so some structures need to have the `Send` and `Sync`
traits.
But these traits are not available for all necessary websys-modules!
So I came up with the idea of linking all modules using a `Broker` system.

In short, all input and output for a module are defined as messages.
Then each module handles incoming messages and produces outgoing messages.
Modules can be linked together by defining `Translators` that take messages
from one module and translate them into messages for the other module.

All message-passing is done asynchronously and doesn't need intervention by
the programmer. 
However, this means that tests sometimes need to wait for all messages to be
transmitted, before the results are available.

## Example

Given the [router-broker](../flmodules/src/router/broker.rs) with its two message-types:
- input: `RouterIn`
- output: `RouterOut`

The [webproxy-broker](../flmodules/src/web_proxy/broker.rs) has similar two message-types:
- input: `WebProxyIn`
- output: `WebProxyOut`

For the webproxy to be able to use the router, it has to connect to the input and output
of the router.
This happens with the following line:

```rust
  web_proxy
      .add_translator_link(
          router,
          Box::new(Self::link_proxy_router),
          Box::new(Self::link_router_proxy),
      )
      .await?;
```

The `link_proxy_router` and `link_router_proxy` translate between the two messages, outputting
`Option`s of the destination message.
This allows to connect the output of the webproxy with the input of the router, and the output
of the router with the input of the webproxy.

Now every time the router receives a message from the web, it will output it with a `RouterOut`
message.
All connected brokers to the router will receive this `RouterOut` message, and translate it into
their internal messages.
The broker system will call the appropriate handlers to create output messages from the modules,
which will then be passed to `RouterIn`, and back to the network.
This all happens in the background, both for `libc` and `wasm` compilation.