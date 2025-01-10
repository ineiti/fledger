# Fledger Loopix Module

## broker.rs
Broker for loopix messsages, it handles starting correct threads for each role and emitting messages to other brokers.

## messages.rs
Main file for loopix messager processing, it handles the processing of messages according to the role of the node, and sends the messages to the loopix broker

## sphinx.rs
Wrapper for sphinx packets, enabling serialization etc

## storage.rs
Storage of all information related network topology, the loopix role have different storages.

## config.rs
Loopix parameters are stored in this file, it can be directly passed to the broker to create a loopix node.
Note, there are many functions starting witht the name default_with, these are used to create network topologies easily for experiments.

## mod.rs
Exports all Loopix files and contains declarations of loopix metrics.

## core.rs
Core functionality of loopix nodes, common to all roles

## client.rs, mixnode.rs, provider.rs
Role specific functionality, each node processes and handles messages differently.