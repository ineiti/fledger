# Work in progress

This file is in place of github issues, as currently I'm mostly developing on my own.

## Current high-level goal

- Store a webpage in fledger

## Current concrete goal

### User-facing

Implement [DHT_STORAGE.md](./DHT_STORAGE.md):
- add editing of new pages
- show pages on the bottom

### Signalling server

### DHT_storage

TODO:
- DHT_routing: check disconnection of nodes when no pings received - doesn't seem to disconnect
- add own Flos to DHTConfig.owned
- verify Flos when they enter the system

### Testing

# TODO

## Features

DHT_Storage:
- when new FloMetas enter the system, check which are the most probable to be kept:
  - choose the closest (with the highest depth) not-yet-stored Flos for synching
- Add a timeout to FloCuckoos when they are purged from the system
- Add a timeout for Flos to purge them from the system

DHT_Router:
- KBucket.active is only be populated once a node has been confirmed.
  - Needs more testing if nodes fail and how they will be replaced
    This definitely doesn't work currently
- possible extension to allow for indpendant realms:
  - active nodes are stored in a global vec
  - each realm-kademlia looks in these nodes first to populate the buckets

## Bugs

- flmodules/web_proxy has horrible error handling - too many `expect`s
  - show an error if no nodes are connected
- UI chat should scroll to the bottom when showing the first message.
- flarch/webrtc errors - perhaps not necessary to fix if matchbox works:
  - Find why the network stalls after some time
  - Doesn't work in EPFL network

## Cleanups / improvements

- remove all the logging comments
- update documentation `flmoduls::network`
- serde_yaml is deprecated
  - use serde_yaml_ng with singleton_map_recursive
- use matchbox from https://github.com/johanhelsing/matchbox
- Clean up broker:
  - Remove `Destination::{All,Others,This}` - Test it
    only `Destination::All` is ever used
  - Replace `process` with `async-task`
    Not sure what is up with that. `process` is a method from `Broker`, while `async-task` is a crate
    Probably instead of calling `Broker.process` use an `async-task` to call pending messages

## Reaching out

Added fledger to blog: https://ineiti.ch/projects/fledger/

# Some things done

### Done
- Flo
  - Uses real signatures and verifications now
- DHT_storage
  - Working version with flos stored in the nearest nodes and automatic synchronisation
    with other nodes.
- Crypto
  - Have working `Signer`s, `Verifier`s, `Badge`s, and `Condition`s.
- DHT_router
  - usable with enough functionalities
- Store yaml files as .yaml instead of .toml
- Flos:
  - Make different updates for data change and for rules change
  - Clean up the mess - it's still too much stuff calling criss-cross each other
  - Removed Rules and ACE
- Test in `flmodules/test/webpage.rs`:
  - complete, including cuckoos, limited memory.
- Make the following tests work again:
  - `make cargo_test`
  - `examples/ping-pong`
  - `test/webrtc-libc-wasm`
- Where are the Verifiers stored? In the DHT, like others
- Store all necessary data for verification in the Flo itself
- flnode/src/node.rs changes:
  - instead of calling `update`, use the `template` version with a tap and a watcher
  - add the timer as an argument to the brokers which need it and remove the `add_timer` method
- rewrite the flmodules/src/\*/broker.rs :
  - move dht_storage way of broker.rs/message.rs to other brokers, too
- Look at all TODOs
- make all tests pass again
- also compile for wasm
- replace `add_subsystem(Subsystem::Handler` with `add_handler`
- `BrokerIO`
  - be renamed to `Broker`? Yes
  - look which `add_translate` are actually used
    -    4 add_translator_dire
    -    7 add_translator_link
    -    4 add_translator_i_ti
    -    19 add_translator_o_ti
    -    4 add_translator_o_to
  - use `add_translator_link`
- unify `flmodules/*/broker.rs`:
  - make sure no trailing `.into()` for the messages are left
- unify `flmodules/*/broker.rs`:
  - naming: is it a verb? Subject?
  - rename `(From|To)Router` and `(From|To)*` into `Router(In|Out)` and `*(In|Out)`
  - add `type BrokerSome = Broker<SomeIn, SomeOut>`
  - always return a structure
    - should it `add_translate` with a `Self::translate...` method, or in-line with `Box::new`?
  - same functionality:
    - structure creates broker and `add_handler` the `message` structure
    - `message` structure takes zero or more of: config, DataStorage, `tokio::sync::watch<Stats>` and is a `Subsystem::Handler`
    - `core` does the core business
- [flmodules::random_connections::messages] 0x6000007c4ab0 Dropping message to unconnected node f88f09414a048f67
  - fixed it, but there might still be some random droppings around. Keeping the log for the moment.

# Dates

2024-09-12:
- update to latest versions of wasm libraries

2024-09-09:
- Clean up broker / network:
  - change enums with `((a, b))` arguments to simple `(a, b)` arguments
  - remove `Destination` from `SubsystemListener`

2022-05-29:
- make github workflows pass
- Update docker
- correctly interpret old config data

2022-05-24:
- DISPLAY: flbrowser displays last ping as descending, but it should increase
- BUG: messages get deleted after restart
- CLEANUP:
  - move `NodeData` into `Node`
  - `npm audit fix` in flbrowser
  - rename `DataStorage` and perhaps move it back to flarch?

2022-05-22:
- Finalize usage of broker-modules in `Node` and `Network`:
  - Create a `WebRTC` and a `Websocket` broker-module
  - Pass these broker-modules to `Network`
  - Pass a `Network` to the `Node` instead of the WebRTC and Websocket spawners
  - flnet-wasm and flnet-libc should directly offer a `Network` for the specific arch

2022-05-21:
- cleanup `flutils`:
  - move `broker` to flmodules
  - move `arch` to `impl`, but rename `impl` to `arch`, and `arch` to `tasks`
- update READMEs

2022-05-18:
- Updated version to 0.6.0
- Cleaned up Cargo.tomls
- Started updating READMEs
- flnet-wasm includes old parking-lot crate because of wasm-timer - but don't want to replace it...

2022-05-16:
- Finish porting of old fledger code:
  - Add flnode binary
  - Add webpage
- update web
- Add connection-type again
- ping-timeout now correctly disconnects nodes
- Reconnection between two browsers doesn't work when quickly reconnecting -> probably difficult to fix
- Check if it's possible to use flmodules::broker::Async instead of cfgattr -> No

2022-05-13:
- added test/fledger-node to test quickly reconnecting between webrtc-wasm

2022-05-12:
- fix web_rtc_libc to remove callbacks in reset
- Fixing part of WASM
- change ping-module:
  - remove "waiting nodes" and just ping every node that is under a certain threshold
  - add counting of pings
- replace local "femme" with fixed upstream

2022-05-11:
- Fast reconnection between to CLIs does not work - "DataChannel is not opened" 

2022-05-05:
- Display some of the statistics in the browser
- Reconnection two CLIs doesn't work
- Fix browser/src/lib.rs to work with new code

2022-05-04:
- run browser without panics

2022-05-03:
- browser/src/lib.rs compiles now

2022-05-02:
- Finish fledger-cli
  - Check why gossip module does excessive messaging
- Add web
  - check why the flmodules cannot be used with wasm - probably needs the proper #[cfg_attr] annotations

2022-04-29:
- Fix TODO in broker with regard to wrong registration of subsystem
- Rewrote random connection module

2022-04-24:
- Finish DataStorageFile
- Add configuration options to fledger CLI
- Available nodes are not updated, so a node will not try to connect to a new node
- follower drops messages because it's not correctly connected - but the gossip event module
  still sends a message - why? Does the random module send out a ListUpdate before it has the
  node connected?

2022-04-23:
- add ping-module
- read and write gossip.storage
- Check why it doesn't correctly connect to the signalling server

2022-04-22:
- flnode/test/gossip_2 doesn't work anymore
- link timer
- add module-field in node messages at random

2022-04-21:
- Added flmodules and flnode crates
- Link it all together

2022-04-15:
- Check dependencies in Cargo.toml and clean up
- Clean up wasm and libc webrtc implementations

2022-04-14:
- follow TODOs

2022-04-13:
- First working inter-wasm-libc communication
- Separated wasm and libc implementations in flnet
- re-think WebSocketConnection::send - should it be async?
- Test libc webrtc implementation against wasm implementation
  - Setup new webrtc connection
- Find out why the ice candidate strings are not recognized
- Pass disconnect event in libc-webrtc

2022-03-16:
- Test libc websocket implementation
- clean-up flnet-libc/tests/websocket

2022-02-05:
- Remove wasm from node
- Cleanup network.rs, signal.rs, types.rs, to be able to move them easier.
- move things around

2022-02-01:
- starting to cleanup network, broker, and types, before the move of the different parts

2022-01-29:
- Extend the chat using gossip-messaging instead of the centralized "oracle" nodes
- high-level: Explore a framework for having multiple modules interacting with each other
- high-level: Propose a set of modules that can work together (random-connections, gossip-events)
- for the unique messages: also encode the time in the msgID, so that an
  updated unique message is also transmitted. Currently the msgID == nodeID,
  so if the 'created' field is updated, the message is not retransmitted.
- clean up names: raw::gossip_chat::message::Message
  - gossip_chat -> gossip_events
  - message::Message* -> event::Event*
- add a node-info storage to the gossip-chat or random-connection
  + replace TextMessages by a new Message with a category type
  + rewrite gossip_chat to use the new Message type
  + add node-info to the Message
  + use node-info from the Message to display the chat messages

2022-01-27:
- get texts loaded from browser storage
- re-enable display of node names (currently it's only the IDs)
- make sure config gets loaded on browser, too - currently it seems to be ignored
- debug real-network failure for new messages

2022-01-26:
- debug connection failures
- write tests
  - test if full connectivity is given
  - test if messages get correctly gossipped

2022-01-19:
- started test-framework with network dummy and first communications

2022-01-18:
- hook up the Network structure to the common-crate
  - fetch chat messages from modules
  - load messages from before

2022-01-09:
- finished implementing the message implementations of the raw modules
- use StorageData
- use 2 * log(n) connections in the random-connections handler

2022-01-08:
- set up first communications within the modules
- created Connections structure to interface with the network layer
- not really sure if it's all worth it...

2022-01-05:
- moved modules to their own crate with 'raw' and 'message' modules
- started working on the 'gossip' module
- next steps: finish 'gossip' module as a generic module, then add the 'message' code
