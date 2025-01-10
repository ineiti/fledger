# Fledger - the fast, fun, easy ledger

## Student Project 24
- The loopix module implemented for this project and be found in ./flmodules/src/loopix, see readme on [here](https://github.com/fledg-re/flmodules/src/loopix/README.md).
- The scripts that were used to run experiments as well as data processing can be found in ./mergetb: [here](https://github.com/fledg-re/mergetb/README.md).
- The latex for the report can be found and compiled in [here](https://github.com/fledg-re/mergetb/blob/main/report/).

### Runnning the code
Rust  binary can be built with the following command:
```bash
cd cli/fledger && cargo build -r
```

Note that the main is configured to run a simulation with multiples node and will not work as a single node. A single node can be run with the following command:
```bash
  ./target-common/release/fledger --config <directory to save configs> \
    --name <name of the node> \
    --n-clients <number of loopix clients> \
    --duplicates <number of duplicates> \
    --path-len <path length> \
    --retry <number of retries> \
    --start_loopix_time <start time> \
    --save_new_metrics_file <save new metrics file> \
    --token <token> 
```
where token is api key for ipinfo.io.

To run multiples nodes, you can use the the script:
```bash
./start_simul.sh
```

### Changes to Fledger
Aside from the loopix module, and the cli/src/main.rs minor changes have been made to the code.

Notably there are some changes int the web proxy module and docker files.


## Fledger
Fledger's main goal is to create a web3 experience in the browser without the
need for proxies.
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
- DeFi - the security guarantees to handle 1e9 US-$ and more will never be there
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
- [flnode](./flnode) - a generic implementation of a fledger-node, mostly connecting all
  _flmodules_ together and with the network
- [flarch](./flarch) - architecture dependant implementations for some async tools
- [flarch_module](./flarch_module) - macro for easier definitions of `async_trait(?Send)`

### Binaries

The following components are available that are used to create the fledger-binaries:
- [Command Line Interfaces](./cli) - command line binaries
  - [Signalling server](./cli/flsignal) - the signalling server for the WebRTC connections
  - [Fledger node](./cli/fledger) - a fledger node implementation for the command line
- [Browser implementation](./flbrowser) - web-frontend using the wasm-implementation

### Implementations and Tests

These are the actual implementations of the WebRTC and Websocket for wasm and libc:
- [WebRTC and Websocket implementations](./flarch) - wasm and libc implementations for WebRTC and Websockets
- [Test directory](./test) - several implementations for testing
- [Example directory](./example) - example for how to use flarch

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

Supposing you have [devbox](https://www.jetify.com/devbox/docs/installing_devbox/) installed, you can run:

```bash
devbox run fledger
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

# Developing

If you want to help with developing, please use
[devbox](https://www.jetify.com/devbox/docs/installing_devbox/)
to have the same development environment as the other developers.
Once you install `devbox`, you can get a shell with

```bash
devbox shell
```

Once the shell is started, you can run `Code` to get a VisualCode which uses the rust
version of devbox.
I suggest you use the `1YiB.rust-bundle` extension in VisualCode, which makes it easier
to use rust.

# License

This project is licensed under MIT or Apache2.
