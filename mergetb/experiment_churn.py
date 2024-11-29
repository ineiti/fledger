from mergexp import *
import os
import random
import time
import threading

# Configuration parameters
path_length = 3
retry = 0
churn_interval = 10  # Interval (in seconds) for churn cycles
churn_probability = 0.5  # Probability of a node leaving during a churn cycle

# Calculate the total number of nodes needed
total_nodes = path_length**2 + path_length * 2

# Define the network topology object
net = Network('loopix', addressing='ipv4', routing='static')

# Create the required number of nodes
nodes = [net.node(f"node_{i}") for i in range(total_nodes)]

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
signal_node.add_ip(f"{signal_ip}/24")

for node in nodes:
    net.connect([signal_node, provider])
    # net.connect([provider, mix], latency='15ms', bandwidth='500Mbps')

# Define binaries to execute
signal_binary = "./target-common/release/flsignal"
fledger_binary = "./target-common/release/fledger"

# Validate binaries exist
if not os.path.exists(signal_binary) or not os.path.exists(fledger_binary):
    raise FileNotFoundError("Required binaries not found. Please build them before running.")

# Start the signaling node
signal_node.execute(f"{signal_binary} -vv")

# Assign IP addresses 
for i, node in enumerate(nodes):
    node.add_ip(f"192.168.1.{i+2}/24")

    name = f"NODE_{i:02d}"
    config_dir = f"simul/{name}/"
    os.makedirs(config_dir, exist_ok=True)

    # Add arguments based on the node index
    path_len_arg = f"--path-len {path_length}" if i == 0 else ""
    retry_arg = f"--retry {retry}" if retry > 0 else ""

    # Execute the fledger binary on each node
    node.execute(
        f"{fledger_binary} --config {config_dir} --name {name} -vv -s ws://{signal_ip}:8765 {path_len_arg} {retry_arg}"
    )

# Function to simulate churn in mixnodes
def simulate_churn(mixnodes, interval, probability):
    while True:
        for mix in mixnodes:
            if random.random() < probability:
                # Stop the node
                print(f"Stopping node: {mix.name}")
                mix.stop()
                time.sleep(1)  # Ensure some downtime
                # Restart the node
                print(f"Restarting node: {mix.name}")
                mix.start(f"{fledger_binary} --config simul/{mix.name}/ --name {mix.name} -vv -s ws://{signal_ip}:8765")
        time.sleep(interval)

# Start a thread for churn simulation
churn_thread = threading.Thread(target=simulate_churn, args=(mixnodes, churn_interval, churn_probability), daemon=True)
churn_thread.start()

# Make this file a runnable experiment
experiment(net)
