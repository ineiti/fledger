# Fledger - the fast, fun, easy ledger

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
- Data storage using a distributed hash table
- Proof-of-participation to distribute _Mana_ to participants (instead of miners)
- Different consensus layers, depending on the need: implicit trust, ledger based

The goal is to be able to serve both short-term online users like browsers,
but also long-term users who have a server where they can run a fledger node.
The system is set up to be 0-configuration and fast.

What it will never do:
- DeFi - the security guarantees to handle 1e9 US-$ and more will never be there
- HODL - Fledger's Mana is not made to be held, but will disappear over time

## State of Fledger

As of February 2025, fledger has:
- a generic WebRTC network library
- gossip-based distribution of messages
- data storage using distributed hash tables
- a simple cryptographic layer for various types of signatures, including
  "N out of T" signatures

The code is written in a way that it can run in a wasm environment on
browser or node, as well as in a libc environment.
Some of the code can also be reused in other projects.

### Student Semester Projects

With the support from prof. Bryan Ford's [DEDIS](https://dedis.epfl.ch) lab, one semester
student project was finished in Autumn '24, and a new one started in Spring '25:

- Derya Cögendez worked on [Churning Mixers](https://c4dt.epfl.ch/wp-content/uploads/2025/01/2025-01-derya_cogendez_churning_mixers.pdf) with the experimental repo here: [student_24_fledger](https://github.com/dedis/student_24_fledger)
- Yohan Max Abehssera started to work in Feb. 2025 on `Fair Sharing` to implement a tit-for-tat
  sharing mechanism for the [DHTStorage](./flmodules/src/dht_storage/) module

## Project Directory

### Library Crates

A set of _shared_ crates implement the basic functionality, without the actual implementation
of the network code:

- [Decentralized modules](./flmodules/) - modules that are usable for decentralized projects
- [flnode](./flnode) - a generic implementation of a fledger-node, mostly connecting all
  _flmodules_ together and with the network
- [flarch](./flarch) - architecture dependant implementations for some async tools
- [flmacro](./flmacro) - macro for easier definitions of `async_trait(?Send)`

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
- Create storage nodes that can serve data
- Create a minimum consensus for people to try it out in the browser at
https://web.fledg.re
- Create a nicer display of the chat, perhaps with markdown display of messages
- Add WASM smart contracts
- Add sharding to have more than one worker chain

# Running it

The simplest way to run it is to go to https://web.fledg.re and follow the
instructions.

## Running it on a server

### Using Docker

If you want to use docker, you can download the `docker-compose.yaml` file and use this to
run two nodes on your server:

```bash
curl https://raw.githubusercontent.com/ineiti/fledger/refs/heads/main/examples/docker-compose/docker-compose.flnode.yaml
docker compose up -d
```

It will initialize the servers, connect to the network, and start participating in sharing the data.

### Using Devbox

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

## Add new Modules

If you want to add a new module, you can use the [template](./flmodules/src/template/) module
by copying it, and changing the name.
You will find information about how to add a new module in the [flmodules/README.md](./flmodules/README.md).

# License

This project is licensed under MIT or Apache2.
