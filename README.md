# WebRTC example

Creating a first example for Fledger, using three components;
- common - holding all common definitions, like structures and logic
- wasm - the web part using yew
- signal - what will be run on the server

The goal is to be able to:

1. Run signal-CLI on the server (fledger.io)
1. Open the wasm part, where every opening in the browser
  - contacts the CLI on the server
  - sends the info necessary to connect over WebRTC
  - loops
    - retrieves all other wasm clients
    - contacts all other clients

# Running it

First run the cli:

```
cd signal
cargo run
```

Then use the IP of the server in the wasm/src/node.rs::start method.

Finally run
```
cd wasm
make serve
```

And connect locally to your server on http://localhost:8080

# Roadmap
- make CI/CD to deploy to fledg.re
  - running signalling server
  - running a node-wasm client
- make a somewhat nice UI
- move the notion.io text here
- auto-run node to connect through Server
- implement linux-network code to add WebRTC to server

# Changelog

- 0.1 - 2021-02-05
  - add ICE connection through the server
    - use websockets to connect to server
    - implement connection in Node
- 0.0 - 2020-12-xx
  - start the idea
