# Fledger - the fast, fun, easy ledger

The main goal of Fledger is to have a blockchain with nodes that are so
lightweight that you can run them in the browser.
When running a node, each user must be able to receive _Mana_ in a reasonable
timeframe (minutes), so she can use the ledger.

For a more thorough introduction to the goals of fledger, see here: [Fledger Docs](https://fledg.re)

A first use-case of Fledger is to have a decentralized webserver:
1. A user clicks on the link to the website
1. The browser downloads the fledger code
1. Connecting to fledger, the browser gets the data for the website and displays it
1. While the user reads the website, the browser participates in fledger and
pays for the view

But of course there are many more things you can do:
- Decentralized user management like [DARCs](https://www.c4dt.org/article/darc/)
- Smart contract execution using WASM contracts
- General storage / backup service

On the technical side, Fledger will have:
- BFT consensus using ByzCoinX
- Proof-of-work using transactions instead of hashing
- Sharding inspired by OmniLedger

So the goal is to be able to serve both short-term online users like browsers,
but also long-term users who have a server where they can run a fledger node.
The system is set up to be 0-configuration and fast.

What it will never do:
- DeFi - the security guarantees for 1e6 US-$ and more will never be there
- HODL - Fledger's Mana is not made to be held, but will disappear over time

## State of Fledger

Currently the following components are available:
- [Signal server](./cli/signal) - for the webRTC rendez-vous
- [Web node](./web) - for running the node in a browser
- [CLI node](./cli/node) - for running a node in a CLI
- [Node](./common) - the actual code for the Fledger nodes

As a first step, the WebRTC communication has been set up.
This works now for Chrome, Firefox, and Safari, as well as in the CLI using
node.

## Next steps

The following next steps are in the pipeline:
- clean up network and make crates:
  - for signalling-server
  - for network-layer
  - for wasm and linux client

- Create a minimum consensus for people to try it out in the browser at
https://fledg.re
- Link server nodes with browser nodes
- Create two chains: identity chain for nodes, and a first worker chain
- Add WASM smart contracts
- Add sharding to have more than one worker chain
- Create storage nodes that can serve data

# Running it

The simplest way to run it is to go to https://fledg.re and follow the
instructions.

## Running it on a server

Get the `docker-compose.yaml` and run it:

```bash
wget https://github.com/ineiti/fledger/raw/main/docker-compose.yaml
docker-compose up -d
```

This will create a new file called `fledger.toml` in the `fledger` directory
that contains the private key of your node.
Do not lose this private key, as it is used to move around the Mana you get.
The only time you need it will be once the server <-> browser connection will
be set up.

## Running it locally for testing

If rust, wasm-pack, and npm is installed in a recent version, you can simply
run it locally by calling:

```bash
make serve_local
```

# Changelog

```
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