# Random Connections

This module chooses some random nodes to set up connections.
It is based on an erroneous calculation of how many random connections are needed
to create a fully connected graph in the nodes:
the caluclation is based on _completely random_ connections, while this module
in fact sets up random connections _per node_, which is a different measure.

2024-09: @ineiti hopes to do a write-up of best number of connections to have a
fully connected graph...