# Broker Templates

The [Broker] structure is used in danu to solve the problem of
message passing in an asynchronous environment.
It is a big wrapper around pairs of [std::sync::mpsc] channels
which send and receive messages.
[Broker]s can be connected together to build complex systems,
where the internal state of each element is hidden from the
other modules.
This module has two templates for different complexities of
[Broker]s:

- [Simple](./simple/README.md) has a single input/output queue
- [Multi](./multi/README.md) takes other brokers as input and
channels them to an internal message [Broker]
- [Persistent](./persistent/README.md) shows how to add configuration
and persistent storage to a module

It is to be noted that some of the modules in [flmodules](../README.md)
also have a `core.rs` file which implements more complex behaviour.
This is for easier unit-testing.
