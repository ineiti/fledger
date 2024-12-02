from mergexp import *
import os

# Configuration parameters
path_length = 3
retry = 0

# Calculate the total number of nodes needed
total_nodes = path_length**2 + path_length * 2

# Define the network topology object
net = Network('loopix', addressing==ipv4, routing==static)

# Create the required number of nodes
nodes = [net.node(f"node-{i}") for i in range(total_nodes)]

# Define groups dynamically
clients = nodes[:path_length]
providers = nodes[path_length:path_length*2]
mixnodes = nodes[path_length*2:] 

# Connect clients and providers
for client in clients:
    for provider in providers:
        # net.connect([node1, node2], latency='10ms', bandwidth='1Gbps')
        net.connect([client, provider])

for provider in providers:
    for mix in mixnodes:
        # net.connect([provider, mix], latency='15ms', bandwidth='500Mbps')
        net.connect([provider, mix])

signal_node = net.node("SIGNAL")
signal_ip = "192.168.1.1"

for node in nodes:
    link = net.connect([signal_node, node])

    # net.connect([provider, mix], latency='15ms', bandwidth='500Mbps')
link[signal_node].socket.addrs = ip4(f'{signal_ip}/24')

# Make this file a runnable experiment
experiment(net)
