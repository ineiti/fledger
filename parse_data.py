import re
import json
import os
import yaml


def get_latency(log_file_path, data):
    time_pattern = re.compile(r"Total time for request: (\d+)")

    try:
        with open(log_file_path, 'r') as file:
            log_data = file.readlines()
    except IOError as e:
        print(f"Error opening file {log_file_path}: {e}")
        return data

    for line in log_data:
        match = time_pattern.search(line)
        if match:
            data["total_times"].append(int(match.group(1)))

    return data

def get_latency_data(data_dir, dir):
    log_file_path = os.path.join(data_dir, dir, 'logs.txt')
    output_json_path = os.path.join(data_dir, dir, 'data.json')

    data = {
        "total_times": [],
    }
    
    data = get_latency(log_file_path, data)
    
    with open(output_json_path, 'w') as json_file:
        json.dump(data, json_file, indent=4)
    
    print(f"Log data written to {output_json_path}")

    return data

def get_n_storages(data_dir):
    pattern = re.compile(r"loopix_storage_\d+\.yaml")

    matching_files = [f for f in os.listdir(data_dir) if pattern.match(f)]

    count = len(matching_files)
    return count

def count_all_messages(data_dir, dir):

    path = os.path.join(data_dir, dir)

    n_storages = get_n_storages(path)
    total_forwarded = 0
    total_sent = 0
    total_received = 0

    for i in range(1, n_storages + 1):
        yaml_file_path = os.path.join(path, f"loopix_storage_{i:02}.yaml")
        data = count_messages(yaml_file_path)
        total_forwarded += data.get("forwarded_messages", 0)
        total_sent += data.get("sent_messages", 0)
        total_received += data.get("received_messages", 0)

    message_data = {
        "total_forwarded": total_forwarded,
        "total_sent": total_sent,
        "total_received": total_received
    }

    output_json_path = os.path.join(path, 'all_messages.json')
    with open(output_json_path, 'w') as json_file:
        json.dump(message_data, json_file, indent=4)
    
    print(f"Message data written to {output_json_path}")

    return message_data

def count_messages(yaml_file_path):
    print(f"Counting messages in {yaml_file_path}")

    data = {
        "forwarded_messages": 0,
        "sent_messages": 0,
        "received_messages": 0,
    }

    try:
        with open(yaml_file_path, 'r') as file:
            yaml_data = yaml.safe_load(file)

        network_storage = yaml_data.get('network_storage', {})
        
        if 'forwarded_messages' in network_storage:
            data["forwarded_messages"] = len(network_storage['forwarded_messages'])
        else:
            print("No 'forwarded_messages' key found in the YAML file.")

        if 'sent_messages' in network_storage:
            data["sent_messages"] = len(network_storage['sent_messages'])
        else:
            print("No 'sent_messages' key found in the YAML file.")

        if 'received_messages' in network_storage:
            data["received_messages"] = len(network_storage['received_messages'])
        else:
            print("No 'received_messages' key found in the YAML file.")

    except FileNotFoundError:
        print(f"Error: File not found at {yaml_file_path}")
    except yaml.YAMLError as e:
        print(f"Error parsing YAML file: {e}")

    return data


def get_directories(directory_path):
    directories = [d for d in os.listdir(directory_path) if os.path.isdir(os.path.join(directory_path, d))]
    return directories

def main():
    simul_data_dir = os.path.dirname('./simul_data/')
    configs = ["lambda", "max_retrieve", "mean_delay", "path_length", "time_pull"]
    aggregated_data = {
        "latency": {},
        "messages": {}
    }

    for config in configs:
        data_dir = os.path.join(simul_data_dir, config)

        aggregated_data["latency"][config] = {}
        aggregated_data["messages"][config] = {}

        for dir in get_directories(data_dir):

            aggregated_data["latency"][config][dir] = get_latency_data(data_dir, dir)
            aggregated_data["messages"][config][dir] = count_all_messages(data_dir, dir)


    big_json_path = os.path.join(simul_data_dir, 'aggregated_data.json')
    with open(big_json_path, 'w') as big_json_file:
        json.dump(aggregated_data, big_json_file, indent=4)

    print(f"Aggregated data written to {big_json_path}")

if __name__ == "__main__":
    main()
