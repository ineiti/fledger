from mergexp import *
import os

path_length = 2

# Calculate the total number of nodes needed
total_nodes = path_length**2 + path_length * 2

# Define the network topology object
net = Network('loopix', addressing==ipv4, routing==static)

# Create the required number of nodes
nodes = [net.node(f"node-{i}", image=='2004', memory.capacity==gb(2), proc.cores==4) for i in range(total_nodes)]

# Ensure single connections between nodes
for i, node in enumerate(nodes):
    for j in range(i + 1, len(nodes)):
        net.connect([node, nodes[j]], capacity==mbps(500), latency==ms(15))

signal_node = net.node("SIGNAL", image=='2004', memory.capacity==gb(2), proc.cores==4)

# Connect the signal node to all other nodes
for node in nodes:
    net.connect([signal_node, node], capacity==mbps(500), latency==ms(15))

# Make this file a runnable experiment
experiment(net)
