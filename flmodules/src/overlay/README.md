# Overlay

Gives an abstraction to allow for different network overlays, e.g., random_connections, dht_network, loopix, and
others. It defines a basic set of messages and a broker, which can then be extended to work with the different
network connection modules.

Two examples are implemented:
- `OverlayRandom` uses the `RandomConnection` broker to handle the network and forwards messages as appropriate
- `OverlayDirect` uses the `Network` broker to handle the network. One problem with this is that the `Connected`
and `Disconnected` messages from the `Network` broker need to be handled here.

To implement a new example who has the `Available`, `Connected`, `Disconnected` messages, the simplest way is to
copy `OverlayRandom` into a new broker.