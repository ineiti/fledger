# Crypto Wrappers

This crypto wrapper defines the following main structures:

- `Signer` - a generic structure to sign messages
- `Verifier` - a generic structure to verify signed messages
- `Condition` - an AND/OR/NofT combination of verifiers
- `Identity` - an updatable pair of `sign` and `update` Conditions,
  representing identities or cryptographic anchors

An example use-case is the following:

- When a node starts, it creates a `Signer`, and a `Identity` with a `Condition`
 pointing to that `Signer`. This allows the node to do key rotation by updating
 its `Identity`
- A user wants to create a website with many objects, so they create:
  - one or more nodes with the corresponding `Identity`s. 
    A CLI tool to update the page also acts as a node.
  - a `WebPageT: Identity` with an OR of all nodes and CLIs `Identity`s
  - a `WebPageAR: ACE` with rules like `update_object`, `add_object`, `rm_object`,
   pointing to the `WebPageT`
- Now the user can create objects and point them to the `WebPageAR`, allowing them to:
  - control all objects from any node
  - update the `WebPageT` if nodes join or go away
  - update the `WebPageAR` if part of the rules should apply to other `Identity`s. An example
    could be a rule to update the `TTL` of an object, which could be allowed by more nodes
    than the rule to modify an object

## Rule

## Expression

The following structures are defined for signing and updating an expression:

```rust
struct ExpressionSignatureCollector {
    expression_id,
    msg,
    signatures: HashMap<RuleID, Signature>,
    #[serde(skip)]
    ev_cache: Option<watch::Receiver<EVCache>>,
}

type RuleID = Hash(msg|RulePath|SignerID);

struct ExpressionUpdateCollector {
    expression_id,
    current_version,
    new_rules,
    sig: Either<ExpressionSignatureCollector, ExpressionSignature>,
}
```

### Sign

1. `Expression.signature_collector(msg)` -> `ExpressionSignatureCollector`
2. `ESC.sign(Signer)` -> `ExpressionSignatureCollector`
3. `ESC.finalize()` -> Result<(`ExpressionSignature`, `ExpressionSignatureVerifiers`), `ESCError`>

#### Signing Subtrees

There might be a usecase for having more privacy-preserving signatures of sub-trees,
so that the signers cannot see the rest of the `Expression`.
But the original `msg` will still need to be verified in one way or another.

### Verify

1. `Expression.verify(msg, ES, ESV)` -> `Result<(), ESError>`

### Update

1. `Expression.update_message(new_rules)` -> `msg`
or
1. `Expression.update_collector(new_rules)` -> `ExpressionUpdateCollector`
2. `EUC.sign(Signer)` -> `ExpressionUpdateCollector`
3. `EUC.finalize()` -> `ExpressionUpdate`
4. `Expression.update(new_rules, ES)` -> `Expression`
or
4. `Expression.update_finalize(EU)` -> `Expression`

## ACE

In an `AccessRule`, the rules only point to `ExpressionID`.
As such, whatever you want to do with an `AccessRule` needs to have
access to the underlying `Expression`s.
Not sure whether this should be a `HashMap`, or better an asynchronous
`expression_getter`.

### Sign

### Verify

### Update