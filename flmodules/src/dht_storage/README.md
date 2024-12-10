# DHT-Storage

Serves a DHT storage which can hold other DHT storages.
Each DHT storage is controlled by a ledger which makes sure that:
- each node gets mana
- each node pays mana for storing data
- node-names are unique
- no front-running is happening

# First usages

## Names for nodes, groups, and accounts

- Nodes: a node is one fledger instance running on the CLI or in the browser
- Groups: multiple nodes can join together to form a group. Every node can
  be part of 0 or more groups.
  A group can share all the mana, share only part of the mana, or share none at all
- Accounts: can change the configuration of the nodes

## Pages for HTML

Allow everybody to store HTML pages in the system.
Pages are kept depending on the mana attached to them.

# Configuration Files

## fledger_dhts.yaml

A first DHT must be created manually by adding a configuration to the
yaml-file of all the nodes of the ledger, e.g.:

```yaml
v1:
  dhts:
  - name: Root
    id: None # Will be filled in by the node
    spread: 10 # All data will be stored in 10 other nodes
    memory: 100k # A minimum of 100kB must be provided by all joining nodes
    homepage: id # An html storage blob that is displayed when landing on this DHT
    ledger:
      static: true # No automatic member update
      members:
      - id1
      - id2
      - id3
```

When restarting the nodes, they will connect and create an ID for the DHT and
the ledger.

## 