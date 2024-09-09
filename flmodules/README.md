# Raw Fledger modules

This crate holds the raw modules for Fledger.
They are written in a way to be as reusable as possible.
For this, most modules have the following structure:

- `broker.rs` - which contains the code to interact with the other modules
- `module.rs` - with the main code of the module, with at least a `process_msg` method
that inputs a message and outputs a vector of answers
- `data-structures` - implementing the basic functionality of the module.

Currently the following modules are available:

- `timer` sends out one message per second and one message per minute
- `random_connections` takes a list of nodes and randomly selects enough nodes for
a fully connected network
- `ping` uses `random_connections` to send regular messages to the connected nodes
to make sure they answer. If a node doesn't answer in due time, a failure message 
is emitted.
- `gossip_events` exchanges events emitted by the nodes and updates the list. It
works both in active mode - sending new messages to neighbours - as in passive
mode - requesting list of available events from other nodes.
- `network` is the basic networking code to set up a new connection using
the signalling server.
- `web_proxy` allows sending a http GET request to another node, using the other
node as a proxy.

# Adding your own modules

As described in the previous section, you should write your modules in three
parts:

1. Core Logic: persistent data, configuration, methods.
This part does not handle any messages to/from the system, but only provides
the actual algorithm.
This might include cryptographic calculations, caching, updating, and other
tasks necessary for the functionality.
Write it in a way that it can also be used by a non-fledger project.
2. Message Handling: create all messages that this module should receive or
send during its lifetime.
All messages defined here must be self-contained: this part must not depend
directly on any other module's message definitions.
3. Translation: this part defines the interactions with the other modules.
This is mostly done with defining a `Translator` which takes incoming messages
and outputs messges defined in the `Message Handling` file.

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
