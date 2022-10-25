# Raw Fledger modules

This crate holds the raw modules for Fledger.
They are written in a way to be as reusable as possible.
For this, most modules have the following structure:

- `broker.rs` - which contains the code to interact with the other modules
- `module.rs` - with the main code of the module, with at least a `process_msg` method
that inputs a message and outputs a vector of answers
- data-structures - implementing the basic functionality of the module.

Currently the following modules are available:

- `random_connections` takes a list of nodes and randomly selects enough nodes for
a fully connected network
- `ping` uses `random_connections` to send regular messages to the connected nodes
to make sure they answer. If a node doesn't answer in due time, a failure message 
is emitted.
- `gossip_events` exchanges events emitted by the nodes and updates the list. It
works both in active mode - sending new messages to neighbours - as in passive
mode - requesting list of available events from other nodes.

## Broker

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
