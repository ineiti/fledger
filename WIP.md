# Work in progress

## Current high-level goal

- Explore a framework for having multiple modules interacting with each other
- Propose a set of modules that can work together

## Current concrete goal

- Extend the chat using gossip-messaging instead of the centralized "oracle" nodes

### Things to do

- Create a nicer display of the chat, perhaps with markdown display of messages

# Dates

2022-01-05:
- moved modules to their own crate with 'raw' and 'message' modules
- started working on the 'gossip' module
- next steps: finish 'gossip' module as a generic module, then add the 'message' code