import os
import re
import json
import sys
import yaml
from extract_raw_data import get_bandwidth_bytes_or_start_time, counter_metrics_to_extract, metrics_to_extract, create_results_dict, simulation_ran_successfully, get_proxy_request, get_incoming_messages, get_histogram_metrics 

def get_metrics_data(data_dir, path_length, n_clients, results, variable, index):
    if metrics_to_extract[0] not in results:
        create_results_dict(results, metrics_to_extract)

    directory = os.path.join(data_dir, variable)
    metrics_file = os.path.join(directory, f"metrics.txt")
    
    print(f"Getting metrics data from {metrics_file}")
    if os.path.exists(metrics_file):
        with open(metrics_file, 'r') as f:
            content = f.read()
        
        for metric in metrics_to_extract:
            if metric == "loopix_bandwidth_bytes" or metric == "loopix_start_time_seconds":
                get_bandwidth_bytes_or_start_time(results, content, metric, index)


            elif metric == "loopix_number_of_proxy_requests":
                get_proxy_request(results, content, metric, index)

            
            elif metric == "loopix_number_of_proxy_requests":
                get_proxy_request(results, content, metric, index)

            elif metric == "loopix_incoming_messages":
                get_incoming_messages(results, content, metric, index)

            elif metric in counter_metrics_to_extract:
                pattern = rf"{metric}\s+([0-9.e+-]+)$"
                match = re.search(pattern, content, re.MULTILINE)
                if match:
                    results[metric].append(float(match.group(1)))

            else:
                get_histogram_metrics(results, content, metric, index)

            

    return True
          
def main():
    if len(sys.argv) != 4:
        print("Usage: python extract_raw_data.py <data_dir> <path_length> <n_clients>")
        sys.exit(1)

    base_path = sys.argv[1]
    data_dir = os.path.join(base_path, "raw_data")
    path_length = int(sys.argv[2])
    n_clients = int(sys.argv[3])

    results = {}
    variables = [d for d in os.listdir(data_dir) if os.path.isdir(os.path.join(data_dir, d))]
    print(variables)

    signal_log_suffix = "_SIGNAL.log"
    json_suffix = ".json"

    for variable in variables:
        results[variable] = {}
        run_dir = os.path.join(data_dir, variable)
        # runs = [f for f in os.listdir(run_dir) if not f.endswith(signal_log_suffix) and not f.endswith(json_suffix)]

        # print(runs)

        try:
            with open(os.path.join(run_dir, f'{variable}.json'), 'r') as f:
                run_keys = json.load(f)
        except FileNotFoundError:
            print(f"File not found: {os.path.join(run_dir, f'{variable}.json')}")
            continue

        index = 0
        option = 0
        for dir_name in [f for f in os.listdir(run_dir) if os.path.isdir(os.path.join(run_dir, f))]:
            if "_" in dir_name:
                i, j = map(int, dir_name.split("_"))
                index = max(index, i)
                option = max(option, j)
                print(f"Processing directory for i={index}, j={option}")


        for i in range(index + 1):
            run_key = run_keys[str(i)]
            results[variable][run_key] = {}

            for j in range(option + 1):

                results[variable][run_key][j] = {}

                node_dir = os.path.join(run_dir, f"{i}_{j}")
                print(node_dir)
        

                nodes = os.listdir(node_dir)
                for enum_node, node in enumerate(nodes):
                    print(node)

                    node_data_dir = os.path.join(node_dir, node, "data")
                    print(node_data_dir)

                    get_metrics_data(node_data_dir, path_length, n_clients, results[variable][run_key][j], "", 0)

                    
        print(results)
    with open(f'{base_path}/raw_metrics.json', 'w') as f:
        json.dump(results, f, indent=2)

if __name__ == "__main__":
    main()