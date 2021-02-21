# Signalling server for Fledger

This is how the different Fledger instances connect to each other.
Every node connects to a signalling server using websockets.
With this websocket connection, the following can be done:

- node -> server: request a connection to another node
- server -> node: initiate a new connection

The first implementation had only a REST interface, but this makes it difficult
to signal a connection request to a node.
