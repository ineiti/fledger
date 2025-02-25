# Crypto Wrappers

This crypto wrapper defines the following main structures:

- `Signer` - a generic structure to sign messages
- `Verifier` - a generic structure to verify signed messages
- `Condition` - an AND/OR/NofT combination of verifiers, badges, and conditions
- `Badge` - a trait with an ID, a Condition, and a version.

An example use-case is the following:

- When a node starts, it creates a `Signer`, and a `Badge` with a `Condition`
 pointing to the `Verifier` of that `Signer`. This allows the node to do key rotation by updating
 its `Badge`
- A user wants to create a website with many objects, so they create:
  - one or more nodes with the corresponding `Badge`s. 
    A CLI tool to update the page also acts as a node.
  - a `WebPageT: Badge` with an OR of all nodes and CLIs `Badge`s
  - a `WebPageAR: ACE` with rules like `update_object`, `add_object`, `rm_object`,
   pointing to the `WebPageT`
- Now the user can create objects and point them to the `WebPageAR`, allowing them to:
  - control all objects from any node
  - update the `WebPageT` if nodes join or go away
  - update the `WebPageAR` if part of the rules should apply to other `Badge`s. An example
    could be a rule to update the `TTL` of an object, which could be allowed by more nodes
    than the rule to modify an object

## Signer and Verifier

Currently there is an implementation for:

- `Ed25519` for good old EDDSA signatures
- `MlDSA` for different sizes of FIPS 204 compatible signatures

## Condition

## Badge
