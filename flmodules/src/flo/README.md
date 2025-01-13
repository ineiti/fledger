# Fledger Object (Flo)

A Fledger Object (Flo) is the basic data structure in Fledger.
It consists of data and a pointer to a `flmodules::crypto::ACE`.
A Flo can be updated over time, either its data part or the `ACE` part.

It looks like this:

```
- Fledger Object (Flo)
  - ID = Hash(Type | Updates_0 | Updates_1 )
  - Content: (Domain, DHT, Ledger, Mana, Blob)
  - Data: Bytes
  - ACE: Version<AceID>
  - Proof<Change>
```

The `Proof` is defined in `flmodules::crypto` and can be either a list of all past values,
together with a signature on the new value, or a proof that a trusted `Identity` signed
the latest value and the version.

## Access Control Element (ACE)

The Access Control Element (ACE) defines how a Flo can be updated.
There should be a process to inherit some of the Action/Condition
pairs from parent ACEs.

```
- Flo - Access Control Element (ACE)
  - 1..n Rules:
    - Action
      - Flo_type:action - will be imposed on all children
      - action - only for the Flo-ID
    - Condition
      - Signature (given one or more public keys, and/or)
      - Ledger States (time, other Flos)
      - Mana (type of Mana, owner of Mana)
      - Delegation (its own Flo type)
```

## Domain

```
- Domain
  - Has
    - Ledger?
    - DHT?
    - 0..n sub-Domains
```

## Original WIP.md

- Data Storage (DS)
  - Has
    - 0..n Blobs

- Blob
  - Has
    - Data
    - Parents

- Node
  - Has
    - 1..n root-Domains: can be modified by the user
    - Configuration:
      - Mana-Condition

- Ledger
  - Has
    - List of nodes who participate
  - Creates
    - Proofs for pointing to data
    - Global State
  - Uses DHT to store Mana for nodes
  - Uses DHT to store global state of Ledger

- Ledger - Proof
  - Has
    - Flo
    - Signature of nodes

- Ledger - Global State
  
- Mana
  - Has
    - Type

- DHT
  - Uses Mana and Domains to prioritize storage
  - Will only accept a restricted list of Flo-IDs as root parents and refuse
    to store Flos with other or missing root parents.

- Domain
  - Has
    - Ledger?
    - DHT?
    - 0..n sub-Domains

- Fledger Object (Flo)
  - Has
    - ID = H(Type | ACE_0 | Updates_0)
    - Type: (Domain, DHT, Ledger, Mana, Blob)
    - ACE
    - 1..n Updates

- Flo - Updates
  - Has
    - Time
    - Data | ACE: replaces previous Data or ACE
    - Proof of condition: will probably need to include all the parents
      ACEs, or at least a proof from the Ledger that all is OK

- Flo - Access Control Element (ACE)
  - Actions: modify, owner.
  - Has
    - 1..n Rules:
      - Action
        - Flo_type:action - will be imposed on all children
        - action - only for the Flo-ID
      - Condition
        - Signature (given one or more public keys, and/or)
        - Ledger States (time, other Flos)
        - Mana (type of Mana, owner of Mana)
        - Delegation (its own Flo type)
