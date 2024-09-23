# Ledger

This is a simple ledger allowing to have a consensus between a number of nodes.
It supports running multiple ledgers in parallel with different subsets of nodes.

1. Send a new `LedgerIn::NewLedger(LedgerConfig)` to all nodes (this function should not be available
   directly over the network, but somehow gated via another module)
2. All ledger nodes will start communicating with each other using `LedgerOut::Network` and
   `LedgerIn::Network`
3. Once the network is settled (updated), `LedgerOut::Status(LedgerStatus::Live)` will be sent.

# Some details

## Global state

1. The ledger only stores the merkle tree of all the data available.

This is how an update of the state is performed:
1. The client sends
   - the most recent version of its data
   - the proof of the most recent version of its data
   - the requested changes
2. The ledger performs a consensus
3. The ledger outputs the new merkle tree
4. The ledger sends the new state with a low TTL to the network

This means that the ledger itself doesn't hold the global state, but only the merkle
tree of the global state.
In this manner, the ledger reduces the needed storage by a lot.

### Some Questions

- what happens if a client goes offline just after sending its transaction?
Because it will not be able to recover the new state of its account, so it
will be lost!
- can the merkle tree be pruned from time to time?
The ledger could add a TTL to each entry in the merkle tree, and if it reaches
0, prune the entry.
  - if the pruning only happens at given epochs, a client could recover its
  entry by proving that it was available at this epoch, but not at later epochs.

## Account model

