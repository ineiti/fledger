from mergexp import *
import os

path_length = 3

total_nodes = path_length**2 + path_length * 2

net = Network('loopix', addressing == ipv4, routing == static)

nodes = [net.node(f"node-{i}", image == '2004', memory.capacity == gb(2), proc.cores == 4) for i in range(total_nodes)]

signal_node = net.node("SIGNAL", image == '2004', memory.capacity == gb(2), proc.cores == 4)

lan = net.connect(nodes + [signal_node], capacity == mbps(500), latency == ms(15))

experiment(net)
