# Signalling server for Fledger

This is how the different Fledger instances connect to each other.
Every node connects to a signalling server using websockets.
With this websocket connection, the following can be done:

- node -> server: request a connection to another node
- server -> node: initiate a new connection

Using the websocket, two nodes can exchange all data necessary to communicate
using webRTC.
