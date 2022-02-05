# Work in progress

## Current high-level goal

- Create usable crates for a signalling server and webrtc clients

## Current concrete goal

- Remove wasm from node
- Separate wasm and libc implementations in flnet

### Things to do

# Dates

2022-02-05:
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