# Fledger - the fast, fun, easy ledger

Fledger's main goal is to create a web3 experience in the browser.
Once the code starts in your browser, it will connect to other browsers,
and start sharing your diskspace, CPU, and network bandwidth.
No need to 
- install a node on a server, the browser is enough
- buy a token to participate, your node will receive some after 1 minute
- get rich by investing early in a Ponzi scheme

For a more thorough introduction to the goals of fledger, see here: [Fledger Docs](https://fledg.re)

A first use-case of Fledger is to have a decentralized chat-application:
1. A user clicks on the link to the website
1. The browser downloads the fledger code
1. Connecting to fledger, the browser gets the messages and displays them

But of course there are many more things you can do:
- Store a decentralized website
- Decentralized user management like [DARCs](https://www.c4dt.org/article/darc/)
- Smart contract execution using WASM contracts
- General storage / backup service

On the technical side, Fledger has / will have:
- Gossip-based sharing of information
- Proof-of-participation to distribute _Mana_ to participants (instead of miners)
- Different consensus layers, depending on the need: implicit trust, ledger based

So the goal is to be able to serve both short-term online users like browsers,
but also long-term users who have a server where they can run a fledger node.
The system is set up to be 0-configuration and fast.

What it will never do:
- DeFi - the security guarantees for 1e6 US-$ and more will never be there
- HODL - Fledger's Mana is not made to be held, but will disappear over time

## State of Fledger

There has been a big rewrite of the first version of Fledger with the goal to:
- provide libraries for the webrtc-connections
- separate the different functionalities as standalone modules
- start implementing a real gossip-based chat application

The code is written in a way that it can run in a wasm environment on
browser or node, as well as in a libc environment.
Some of the code can also be reused in other projects.

### Library Crates

A set of _shared_ crates implement the basic functionality, without the actual implementation
of the network code:

- [Decentralized modules](./flmodules/) - modules that are usable for decentralized projects
- [Networking](./flnet) - traits for the Websocket and the WebRTC, as well as a generic
  implementation of the networking logic
- [flnode](./flnode) - a generic implementation of a fledger-node, mostly connecting all
  _flmodules_ together and with the network
- [flarch](./flarch) - architecture dependant implementations for some async tools

### Binaries

Currently the following components are available that are used to create the fledger-binaries:
- [Command Line Interfaces](./cli) - command line binaries
  - [Signalling server](./cli/flsignal) - the signalling server for the WebRTC connections
  - [Fledger node](./cli/fledger) - a fledger node implementation for the command line
- [Browser implementation](./flbrowser) - web-frontend using the wasm-implementation

### Implementations and Tests

These are the actual implementations of the WebRTC and Websocket for wasm and libc:
- [WebRTC and Websocket implementations](./flnet) - wasm and libc implementations for WebRTC and Websockets
- [Test directory](./test) - several implementations for testing
- [Example directory](./example) - example for how to use flnet

## Next steps

The following next steps are in the pipeline:
- Create a nicer display of the chat, perhaps with markdown display of messages
- Create a minimum consensus for people to try it out in the browser at
https://web.fledg.re
- Create two chains: identity chain for nodes, and a first worker chain
- Add WASM smart contracts
- Add sharding to have more than one worker chain
- Create storage nodes that can serve data

# Running it

The simplest way to run it is to go to https://web.fledg.re and follow the
instructions.

## Running it on a server

Supposing you have rust installed, you can run:

```bash
cargo run cli/fledger
```

This will create a new file called `fledger.toml` in the `fledger` directory
that contains the private key of your node.
Do not lose this private key, as it is used to move around the Mana you get.
The only time you need it will be once the server <-> browser connection will
be set up.

## Running it locally for testing

If rust, wasm-pack, and npm are installed in a recent version, you can simply
run it locally by calling:

```bash
make serve_local
```

This will run a local signalling server and two nodes that start to communicate.
Additionally you can open your browser and point ot to http://localhost:8080 to
access the node in the browser.

# Changelog

```
- 0.7.0 - 2022-10-??
  - Re-arranging crates and publish
  - Adding examples
  - Moving stuff around to make it easier to understand
  - Add background processing of broker messages
- 0.6.0 - 2022-05-??
  - Rewrite of networking layer
  - Use real gossiped decentralized message passing
  - Offer crates for using parts of fledger directly
- 0.5.0 - 2022-??-??
  - Rewrote big parts of the library and application to make it more modular
- 0.4.1 - 2021-09-16
  - Using thiserror::Error instead of String
- 0.4.0 - 2021-07-27
  - Added signature to the connection with the signal-server, thanks to
      Bolton Bailey <bolton.bailey@gmail.com>
    during the IC3 Hackathon
- 0.3.0 - 2021-04-08
  - More stable everything
  - Clean up a lot of locking issues
  - Fixing issues
- 0.2.3 - 2021-03-04
  - Fix node Running
  - Add docker-compose.yaml
- 0.2.2 - 2021-03-02
  - Run some nodes constantly on https://fledg.re to have a minimum consensus
- 0.2.1 - 2021-02-28
  - Make the https://web.fledg.re a bit nicer and more automatic
- 0.2 - 2021-02-26
  - Simple ping test with the nodes
  - CLI node using headless Chrome
  - Have website https://fledg.re running and pointing to an up-to-date
  Fledger code
- 0.1 - 2021-02-05
  - add ICE connection through the server
    - use websockets to connect to server
    - implement connection in Node
- 0.0 - 2020-12-xx
  - start the idea
```