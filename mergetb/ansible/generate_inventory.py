import random
import sys
import os

def generate_inventory_file(path_length, n_clients, reservation_name):
    num_nodes = path_length + n_clients + path_length * (path_length - 1)
    # SIGNAL_NODE section
    inventory = "[SIGNAL_NODE]\n"
    inventory += f"SIGNAL ansible_host=SIGNAL.infra.{reservation_name}.fledger.dcog ansible_user=dcog ansible_ssh_private_key_file=~/.ssh/id_mrg-0\n\n"

    # FLEDGER_NODES section
    inventory += "[FLEDGER_NODES]\n"
    nodes = [f"node-{i}" for i in range(num_nodes)]
    random.shuffle(nodes)
    for node in nodes:
        inventory += f"{node} ansible_host={node}.infra.{reservation_name}.fledger.dcog ansible_user=dcog ansible_ssh_private_key_file=~/.ssh/id_mrg-0\n"
    inventory += "\n"

    # ALL_NODES section
    inventory += "[ALL_NODES:children]\n"
    inventory += "SIGNAL_NODE\n"
    inventory += "FLEDGER_NODES\n"

    filename = "inventory.ini"

    # Save the inventory to a file
    with open(filename, "w") as file:
        file.write(inventory)

    print(f"Inventory file generated and saved to {filename}")

if __name__ == "__main__":
    # Check if the required arguments are provided
    if len(sys.argv) != 4:
        print("Usage: python script_name.py <path_length> <n_clients> <reservation_name>")
        sys.exit(1)

    # Get the arguments
    path_length = int(sys.argv[1])
    n_clients = int(sys.argv[2])
    reservation_name = sys.argv[3]

    # Generate and save the inventory file
    generate_inventory_file(path_length, n_clients, reservation_name)