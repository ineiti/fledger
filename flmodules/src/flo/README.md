# Fledger Object (Flo)

A Fledger Object (Flo) is the basic data structure in Fledger.
It consists of data and an Access Control Element (ACE).
A Flo can be updated over time, either its data part or the ACE part.

It looks something like this:

```
- Fledger Object (Flo)
  - ID = Hash(Type | Updates_0 | Updates_1)
  - Type: (Domain, DHT, Ledger, Mana, Blob)
  - 1..n Updates
```

The first two updates define the data and the ACE and are included in
the calculation of the ID of a Flo.
Each update has the following structure and updates either the ACE or
the data.
The first two updates have an empty `Proof`.

```
- Flo - Updates
  - Has
    - Time
    - Data | ACE: replaces previous Data or ACE
    - Proof of condition: will probably need to include all the parents
      ACEs, or at least a proof from the Ledger that all is OK
```

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