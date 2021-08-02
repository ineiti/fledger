# Logic

This is the main logic of the Fledger app.
It offers currently two modules:

- statistics of which nodes were online
- serving webpages

## Stats

Every node sends a ping every 3 seconds to all other nodes.
Every node reports the list of available nodes every 30 seconds to the signalling server

## Text Messages

The first _real_ usecase of Fledger. 
An overview is described in the [SHAREIT](../../../../SHAREIT.md) document.
Currently I'm implementing the first version of the proposal.
