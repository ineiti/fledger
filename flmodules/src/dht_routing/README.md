# DHT Routing

As the set of nodes is not completely connected, a routing module is needed.
This one connects to the other nodes in a way which is suitable for the dht_storage module:
the probability of setting up a connection is proportional to the closeness of the node.