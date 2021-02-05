# WebRTC example

Creating a first example for Fledger, using three components;
- common - holding all common definitions, like structures and logic
- wasm - the web part using yew
- cli - what will be run on the server

The goal is to be able to:

1. Run CLI on the server (fledger.io)
1. Open the wasm part, where every opening in the browser
  - contacts the CLI on the server
  - sends the info necessary to connect over WebRTC
  - loops
    - retrieves all other wasm clients
    - contacts all other clients

# Running it

First run the cli:

```
cd cli
cargo build
./cli --server
```

Then use the IP of the server in the

# Roadmap

Next steps of the project:
- add ICE connection through the server
  - add REST endpoints
  - implement connection in Node
- auto-run node to connect through Server
- implement linux-network code to add WebRTC to server
