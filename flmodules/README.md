# Raw Fledger modules

This crate holds the raw modules for Fledger.
They are written in a way to be as reusable as possible.
For this, most modules have the following structure:

- `broker.rs` - which contains the code to interact with the other modules
- `module.rs` - with the main code of the module, with at least a `process_msg` method
that inputs a message and outputs a vector of answers
- `core.rs` - implementing the basic functionality of the module.

The following modules are available:

- [dht_router](./src/dht_router/README.md) implements a message routing system based on kademlia. It can
  send messages directly to connected nodes, to the closest nodes (useful for
  DHT storage), or broadcast to directly connected nodes.
- [dht_storage](./src/dht_storage/README.md) allows to store blobs of data on the nodes, which can be
  updated and read by all other nodes.
- [flo](./src/flo/README.md) defines different data types and how to handle authorization
  to create and update
- [gossip_events](./src/gossip_events/README.md) exchanges events emitted by the nodes and updates the list. It
works both in active mode - sending new messages to neighbours - as in passive
mode - requesting list of available events from other nodes.
- [network](./src/network/README.md) is the basic networking code to set up a new connection using
the signalling server.
- [random_connections](./src/random_connections/README.md) takes a list of nodes and randomly selects enough nodes for
a fully connected network
- [router](./src/router/README.md) is an intermediate layer that contains all messages to be implemented
for current and future communication layers.
Currently it's implemented for `random_connections` and `network`.
- [template](./src/template/README.md) defines three different templates, one for a very simple broker,
  one with a cleaner separation, and a third one with configuration and storage
- [timer](./src/timer.rs) sends out one message per second and one message per minute
- [web_proxy](./src/web_proxy/README.md) allows sending a http GET request to another node, using the other
node as a proxy.

## Dependencies


This is the dependencies between the different modules:

- `Signal` is the root of all control
  - `Network` sets up its connections using `Signal`
    - `RandomConnection` uses the `Network` to set up connections
    - `Router` is an abstraction of the `Network` or the `RandomConnection`
        - `DHT_Router` uses `Router`
            - `DHT_Storage` uses `DHT_Router`
        - `WebProxy` uses `Router`

# Adding your own modules

As described in the previous section, you should write your modules in three
parts:

1. Core Logic: persistent data, configuration, methods.
This part does not handle any messages to/from the system, but only provides
the actual algorithm.
This might include cryptographic calculations, caching, updating, and other
tasks necessary for the functionality.
Write it in a way that it can also be used by a non-fledger project.
If possible, no async should be used in this implementation.

2. Message Handling: create all messages that this module should receive or
send during its lifetime.
All messages defined here must be self-contained: this part must not depend
directly on any other module's message definitions.

3. Translation: this part defines the interactions with the other modules.
This is mostly done with defining `translator` methods which take incoming messages
and output messges for this module.

## Testing

Testing your new module should be done in three steps:

1. Write tests for the core logic.
   Make sure that all the necessary logic functions as it should.
   Test edge-cases, and add more tests as the core logic expands.
   The `Message Handling` part should be as simple as possible, and as
   much of the logic should be here.

2. Then go on to test the `Message Handling` part.
   Create multiple objects of the structure, and let them interact using
   simple simulators.
   In this stage, it might be enough to do all message-passing manually,
   so you have full control over what is going on.
   This also helps to test edge-cases, because it allows you to 'delay' messages
   artificially between modules.

3. At the very end should you start implementing the more end-to-end tests.
   As a first step it is good to use the router simulator in
   [router_simul](./src/testing/router_simul.rs).
   It abstracts the router in a useful way, while still allowing you to have
   some control of the message flow.
   Depending on what you're doing, it might also be worth using the
   [full_simul](./src/testing/full_simul.rs) which has an as-close-as-possible
   simulation of all the messages.

### Full end-to-end Tests

Once all these tests work, you can do real tests with the binaries and set them
up to use a signalling server.
