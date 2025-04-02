# Node Signal

This module allows connecting nodes without the usage of a signalling
server.
Instead of using the signalling server to exchange `PeerMessage`s, it
uses common nodes for this exchange.
It is available as a `BrokerRouter` and uses the `BrokerNetwork` for the
initial connections of the nodes.
For every new node connected, it asks the node for other known nodes,
and then presents those to the attached modules.