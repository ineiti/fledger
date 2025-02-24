# Testing

Here is a list of all testing harnesses in `flmodules/testing` and what they do:

- [flo.rs](./src/testing/flo.rs) - provides a `Wallet` to simplify setting up the signatures for the Flos
- [router_simul.rs](./src/testing/router_simul.rs) - bad copy of NetworkBrokerSimul
- [network_broker_simul.rs](./src/testing/network_broker_simul.rs) - quite complete, creates RouterNodes
- [full_simul.rs](./src/testing/full_simul.rs) - TODO: implement full signal/network/flnode simulation

These are used here:
- in `flmodules`:
  - [dht_storage/broker.rs](./src/dht_storage/broker.rs) - `SimulStorage` - uses `NetworkBrokerSimul` and creates `DHTStorage` - very simplistic
  - [tests/webpage.rs](./tests/webpage.rs) - uses flmodules::network::NetworkBrokerSimul and creates Nodes with wallets
- [flnode/tests/helpers](../flnode/tests/helpers/mod.rs) - `NetworkSimul` - `NodeTimer` - manual processing of messages
