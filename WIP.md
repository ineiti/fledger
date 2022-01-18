# Work in progress

## Current high-level goal

- Explore a framework for having multiple modules interacting with each other
- Propose a set of modules that can work together

## Current concrete goal

- Extend the chat using gossip-messaging instead of the centralized "oracle" nodes

### Things to do

- write tests
- Create a nicer display of the chat, perhaps with markdown display of messages

# Dates

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