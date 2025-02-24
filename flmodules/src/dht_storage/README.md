# DHT-Storage

Creates a DHT storage which can hold [Realm]s containing [Flo](../flo/README.md)s.
A Realm is a configuration of a storage, defining how long it holds the data,
and what the replication factor for the data is.
It uses the [dht_router](../dht_router/README.md) module to contact other nodes and 
exchange data.

## Initialization

Before the DHT-Storage system can be used, it needs to be initialized with a Realm.
You can find an example of how to set it up in [webpage.rs](../../tests/webpage.rs).
For instructions how to set it up on the CLI, you can have a look at 
[fledger](../../../cli/fledger/README.md).
If you want to use it in the browser, go to [fledg.re](https://web.fledg.re) and follow
the instructions there.

# Elements

Here is a short overview of the elements of the DHT-Storage.
For more details, refer to [Flo](../flo/README.md).
- `Realm` is a configuration for one DHT-storage. Each realm is independant of the others.
- `Flo` is a Fledger Object and can store any data. Several pre-defined Flos exist, but most
  usage will be done with the `Blob`.
- `Badge`, `ACE` and `Rules` refer to control structures for the signatures and the verification,
  allowing flexible configuration of the access to the `Flo`s.