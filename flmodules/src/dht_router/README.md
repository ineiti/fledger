# DHT Router

This module offers message sending and receiving of messages using ideas based on 
[Kademlia](https://pdos.csail.mit.edu/~petar/papers/maymounkov-kademlia-lncs.pdf).
Contrary to Kademlia, the DHT_Router does not set up a connection to the closest
destination of the message.
Instead it sets up a list of buckets as in the Kademlia paper to have a restricted
set of nodes with which it talks.
Messages are then actually forwarded through these nodes until they reach destination.
The advantage is that it doesn't need to set up new connections for every destination.
The disadvantage is that the attack surface with regard to dropping messages is much bigger.

The DHT Router offers these three types of messages:
- `MessageClosest` which correspond the most to the original Kademlia messages. It sends
  a message as close as possible to the destination.
- `MessageDirect` is used for replies in the system - it tries to match a node directly,
  but can fail silently if the destination is not reachable.
- `MessageBroadcast` sends the message to all currently connected nodes.

## Performance

As long as all nodes get a full list of all other nodes, the system performs very nice:
as well `MessageClosest` and `MessageDirect` reach the defined targets.
Using the tests, it is shown that even with very short bucket lengths of 1 every message
reaches its target.

However, if some of the nodes only have a partial view of the network, this completely breaks.
In this case, not all of the messages get to the closest or the target node.
But for replies even in this scenario it should still hold:
unless the network changed a lot between the request and the reply, the reply should be
able to find the source node.

# Implementation

The implementation in [kademlia.rs](./kademlia.rs) is split in two structures:
- `Kademlia` to manage adding/removing nodes and finding the relevant buckets
- `KBucket` to implement one bucket of nodes, including calculation of the distance
  and handling the life-cycle of the nodes
