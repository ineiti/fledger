# Crypto Wrappers

The goal of this crypto wrapper is to have two structures:

- `Expression`, allowing AND/OR/NofT combination of Verifiers
- `AccessRules`, a special type of `Expression`

## Sign

1. `Expression.signature_collector(msg)` -> `ExpressionSignatureCollector`

```rust
struct ExpressionSignatureCollector {
    expression_id,
    msg,
    verifiers: Vec<Verifier>,
    signatures: HashMap<RuleID, Signature>,
}

type RuleID = Hash(msg|RulePath|SignerID);
```

2. `ESC.sign(Signer)` -> `ExpressionSignatureCollector`
3. `ESC.finalize()` -> Result<(`ExpressionSignature`, `ExpressionSignatureVerifiers`), `ESCError`>

### Signing Subtrees

There might be a usecase for having more privacy-preserving signatures of sub-trees,
so that the signers cannot see the rest of the `Expression`.
But the original `msg` will still need to be verified in one way or another.

## Verify

1. `Expression.verify(msg, ES, ESV)` -> `Result<(), ESError>`

## Update

1. `Expression.update_message(new_rules)` -> `msg`
or
1. `Expression.update_collector(new_rules)` -> `ExpressionUpdateCollector`

```rust
struct ExpressionUpdateCollector {
    expression_id,
    current_version,
    new_rules,
    sig: Either<ExpressionSignatureCollector, ExpressionSignature>,
}
```

2. `EUC.sign(Signer)` -> `ExpressionUpdateCollector`
3. `EUC.finalize()` -> `ExpressionUpdate`
4. `Expression.update(new_rules, ES)` -> `Expression`
or
4. `Expression.update_finalize(EU)` -> `Expression`
