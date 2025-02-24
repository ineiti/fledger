# Router

Gives an abstraction to allow for different network router algorithms, e.g., random_connections, dht_network, loopix, and
others. It defines a basic set of messages and a broker, which can then be extended to work with the different
network connection modules.

Two examples are implemented:
- `RouterRandom` uses the `RandomConnection` broker to handle the network and forwards messages as appropriate
- `RouterDirect` uses the `Network` broker to handle the network. One problem with this is that the `Connected`
and `Disconnected` messages from the `Network` broker need to be handled here.

To implement a new example who has the `Available`, `Connected`, `Disconnected` messages, the simplest way is to
copy `RouterRandom` into a new broker.

# Timeline with Network

The `network`-module will sooner or later be replaced with another WebRTC implementation, perhaps
with https://crates.io/crates/matchbox_socket, https://crates.io/crates/litep2p, or
https://crates.io/crates/just-webrtc.