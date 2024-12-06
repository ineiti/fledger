import re

# Input file containing materialization data
materialization_file = "materialization.txt"

# Output Ansible inventory file
inventory_file = "inventory.ini"

def parse_materialization(file_path):
    """
    Parses the materialization file to extract node mappings.
    """
    node_ip_mapping = {}

    # Regular expression to match nodes and their metadata
    node_pattern = re.compile(r"(\S+)@(\S+) &\{.*?tunnel_ip:\"(\d+\.\d+\.\d+\.\d+)\"")
    
    with open(file_path, "r") as file:
        for line in file:
            match = node_pattern.search(line)
            if match:
                node_name = match.group(1)
                ip_address = match.group(3)
                node_ip_mapping[node_name] = ip_address

    return node_ip_mapping

def write_inventory(node_ip_mapping, output_file):
    """
    Writes the Ansible inventory file from the node mapping.
    Separates the SIGNAL node into its own group.
    """
    with open(output_file, "w") as file:
            # Write SIGNAL group
            file.write("[SIGNAL_NODE]\n")
            for node, ip in node_ip_mapping.items():
                if node.lower() == "signal":
                    ansible_hostname = f"{node}.infra.tryagain.fledgerfirst.dcog"
                    file.write(f"{node} ansible_host={ansible_hostname} ansible_user=dcog ansible_ssh_private_key_file=~/.ssh/id_mrg-0\n")
            
            # Write ROOT_NODE group
            file.write("\n[ROOT_NODE]\n")
            for node, ip in node_ip_mapping.items():
                if node.lower() == "node-1":
                    ansible_hostname = f"{node}.infra.tryagain.fledgerfirst.dcog"
                    file.write(f"{node} ansible_host={ansible_hostname} ansible_user=dcog ansible_ssh_private_key_file=~/.ssh/id_mrg-0\n")
                               
            # Write FLEDGER_NODES group
            file.write("\n[FLEDGER_NODES]\n")
            for node, ip in node_ip_mapping.items():
                if not node.startswith('ifr'):
                    if node.lower() != "signal" and node.lower() != "node-1":
                        ansible_hostname = f"{node}.infra.tryagain.fledger.dcog"
                        file.write(f"{node} ansible_host={ansible_hostname} ansible_user=dcog ansible_ssh_private_key_file=~/.ssh/id_mrg-0\n")

            file.write("\n[ALL_NODES:children]\nSIGNAL_NODE\nFLEDGER_NODES\nROOT_NODE")

def main():
    # Parse the materialization file
    node_ip_mapping = parse_materialization(materialization_file)
    
    if not node_ip_mapping:
        print("No nodes found in the materialization file.")
        return

    # Write the inventory file
    write_inventory(node_ip_mapping, inventory_file)
    print(f"Inventory file '{inventory_file}' generated successfully.")

if __name__ == "__main__":
    main()
