# Fledger Object (Flo)

A Fledger Object (Flo) is the basic data structure in Fledger.
It consists of data and a history of signatures on previous versions.
A Flo can be updated over time, for both of the `data` field or the `Rules`
field in the `history`.

All data is stored in a `Flo`, even though the [dht_storage](../dht_storage/core.rs)
module puts it in a `FloMeta` structure, adding timestamps and `Cuckoo`s.
To make it easier to work with the different data types stored in a `Flo`,
you can use a `FloWrapper<T>`.
It holds a copy of the original data, as well as the corresponding `Flo`.

The `FloWrapper`s defined are:
- [crypto](./crypto.rs)
  - `FloVerifier` wraps the public key of a keypair
  - `FloBadge` is a [crypto::Condition], an AND/OR combination of `Verifier`s
    to validate something
  - `FloACE` implements an access control system based on rules
- [realm](./realm.rs)
  - `FloRealm` for a realm which holds a DHT configuration
- [blob](./blob.rs)
  - `FloBlob` a generic data storage
  - `FloBlobPage` holds a html page, including its resources
  - `FloBlobTag` points to other tags and Flos

## History

Stores the data necessary to prove that the current version has correctly evolved
from the genesis version.

### Rules

Define how the Flo can be updated, or, using an ACE, what actions can be performed on
the flo.

## Updating

A structure allowing to define a next version of a Flo and to collect signatures.

## Cuckoo

Mostly used for FloBlobTag and FloBlobPage, to allow attaching to existing, well-known
FloBlob* elements.

# Future Flo-types

The following `FloWrapper`s are on the TODO list:
- `FloNode` to represent the public configuration of a node
- `FloLedger` holds the definition of a ledger, to serve as a central
  blackboard
  - GlobalState and History
- `FloMana` represents tokens for the Badge