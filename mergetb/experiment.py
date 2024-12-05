from mergexp import *
import os

# Configuration parameters
path_length = 3
retry = 0

# Calculate the total number of nodes needed
total_nodes = path_length**2 + path_length * 2

# Define the network topology object
# net = Network('loopix', addressing==ipv4, routing==static, experimentnetresolution===True)
net = Network('loopix', addressing==ipv4, routing==static)

# Create the required number of nodes
nodes = [net.node(f"node-{i}") for i in range(total_nodes)]
# nodes = [net.node(f"node-{i}",  metal==True) for i in range(total_nodes)]

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

ip_count = 1

# # Assign a single IP address to the signal node
# signal_node_ip = '192.168.1.1/24'
# signal_node.socket.addrs = ip4(signal_node_ip)

# Connect the signal node to all other nodes
for node in nodes:
    link = net.connect([signal_node, node])
    # Assign unique IPs to other nodes in the same subnet as the signal node
    link[node].socket.addrs = ip4(f'192.168.{ip_count}.1/24')
    ip_count += 1

# net.connect([provider, mix], latency='15ms', bandwidth='500Mbps')
link[signal_node].socket.addrs = ip4(f'192.168.0.0/24')


# Make this file a runnable experiment
experiment(net)
