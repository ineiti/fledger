# Distributed Hash Table Network

Its place in the module system:
- `dht_network` creates the necessary network connections 
    - replaces `random_conn`
    - compatible with `random_conn` to allow for `gossip_events` and `ping` to work
    - configurable with respect to how many bits are in each pool

## Design decisions

The DHT-net can store arbitrary data distributed over many nodes.
The following design decisions have been made:
1. Share storage of data over multiple nodes, so even if one node goes
down, other nodes can serve the data
  - While storing data, all intermediate nodes can decide if they want to
  store the data, too
  - While retrieving data, all intermediate nodes can send their blob back
  to the reader
1. Allow some control of what kind of data you store for other people,
to avoid participating in sharing data you don't like
  - Split data in two parts: pointers and binaries.
  Everybody stores pointers which only hold binary-ID / nodeID pairs.
  Only interested nodes store the binary-blobs.
1. Don't rely on any central system that directs data to one or another node
  - Use a DHT to organise which nodes might hold copies of the data

## Data blob structure

Every data blob has the following structure:

- ID: the hash of the first version of the header and the body. 
This is the unique identifier and will never change throughout
the lifetime of this blob.
- Header:
    - Version: starting at 0 and monotonically increasing
    - Date: unix-timestamp in msec of this blob.
    The version/date pairs must be increasing over the lifetime of a blob.
    A later version cannot have a previous date
    - StorageNode: where this blob should be stored.
- Body:
    - Data: up to 1MB of data for the blob.
    - Type: currently following types are available:
        - pointer: holds a node/ID pair in the Data that shows where the
        corresponding data can be found
        - binary: means the data holds binary data
        - some ideas for more types:
            - mime-type: means the data starts with the mime-type, followed
            by a "/", and then the data in binary
            - tgz: a set of files from a .tar.gz archive
Needed extensions:
- VersionProof: making it possible to check the given version is based upon
an existing version and not just created at random.
The simplest way of doing it is to have a public key in the version 0, and then
using this to verify a signature on every next version.

## Storing and retrieving of data

To store data, the client does the following:
1. create a binary-blob with the data and store it locally
1. create a pointer-blob with the data being its own nodeID / the ID of the binary blob
1. send the pointer-blob to the nodeID of the blob-ID

If somebody wants to retrive the data, they need the binary-blob-ID

## The role of the DHT

# Kademlia implementation

Kademlia
  - Vec<KBucket>
  - RootBucket
    - KTree
      - KTreeNode
    - KBucket