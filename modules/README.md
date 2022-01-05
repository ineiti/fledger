# Fledger modules

The parts of fledger are implemented as modules.
Each module does one task.
In order to be able to re-use these modules in other contexts, each
module is implemented twice:

- once as a `raw` module that only depends on the `NodeID` and `NodeIDs`.
  These implementations need to be as independant as possible from each other.
  They should not expect any specific connection-type or storage type to be present.
- once as a `message` module that needs to implement the `Module` trait.
  This `Module` trait links the module between each other using messages.
  Of course other modules can be used that are not in the `raw` folder.

As a starting point, the following two modules are implemented:

- `random_connections` - which keeps a constant number of nodes connected
and provides some automatic churn of the nodes.
- `gossip` - synchronizes the messages between all known nodes

The following modules are on the list of a possible implementation / wrapper:

- `pot` - implement a proof-of-teamwork mechanism that can be used by other modules
- `onion` - an onion-routing layer
- `storage` - a distributed storage with submodules like
    - `sharing` - for a public file-sharing
    - `backup` - a private file-storage that can be used for backups
    - `web` - storing websites and retrieving them
- `dns` - to map names to IDs
- `poldercast` - as a wrapper around https://github.com/primetype/poldercast
  I thought of using this as the basic networking layer. But I'm not sure it is
  byzantine-resistant enough. Also, it might require too many connections for
  the WebRTC connections.

Although I'm not sure yet which parts should be implemented on a higher level. For example the
following services are probably not in here:

- `chat` - setting up a public/private messaging system
- `web` - instead of having it as a submodule of `storage`, perhaps it is its own service
- `sharing` - if `storage` is generic enough, it can also be used as sharing system