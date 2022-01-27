# Work in progress

## Current high-level goal

- Explore a framework for having multiple modules interacting with each other
- Propose a set of modules that can work together
- Create usable crates for a signalling server and webrtc clients

## Current concrete goal

- Extend the chat using gossip-messaging instead of the centralized "oracle" nodes

### Things to do

- re-enable display of node names (currently it's only the IDs)
  - needs perhaps a node-name-cache
- Create a nicer display of the chat, perhaps with markdown display of messages

# Dates

2022-01-27:
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