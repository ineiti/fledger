# Fledger Object (Flo)

A Fledger Object (Flo) is the basic data structure in Fledger.
It consists of data and a pointer to a `flmodules::crypto::ACE`.
A Flo can be updated over time, either its data part or the `ACE` part.

It looks like this:

- Fledger Object (Flo)
  - ID = Hash( Content | Data | ACE )
  - Content: String
  - Data: Bytes
  - ACE: Version<AceID>
  - History<Change>

The `Version<AceID>` is defined in `flmodules::crypto` and indicates which ACE and which
of its versions are trusted by this Flo.

The `History` is defined in `flmodules::crypto` and can be either a list of all past values,
together with a signature on the new value, or a proof that a trusted `Identity` signed
the latest value and the version.

The different contents of a `Flo` is created using a `FloWrapper<Type>`.
For the current implementation of a simple page-sharing system, the following contents are
implemented:

- `FloACE`, `FloIdentity`, `FloVerifier` for the cryptographic objects used
- `Domain` for the naming system
- `DHT` to store the blobs in a DHT
- `Blob` to store binary data

## Domain

- Domain
  - Has
    - Ledger?
    - DHT?
    - 0..n sub-Domains

## DHT

- DHT
  - Uses Mana and Domains to prioritize storage
  - Will only accept a restricted list of Flo-IDs as root parents and refuse
    to store Flos with other or missing root parents.

## Blob

# Future Flo-types

- Node
  - Has
    - 1..n root-Domains: can be modified by the user
    - Configuration:
      - Mana-Condition

- Ledger
  - Has
    - List of nodes who participate
  - Creates
    - Historys for pointing to data
    - Global State
  - Uses DHT to store Mana for nodes
  - Uses DHT to store global state of Ledger

- Ledger - History
  - Has
    - Flo
    - Signature of nodes

- Ledger - Global State
  
- Mana
  - Has
    - Type

