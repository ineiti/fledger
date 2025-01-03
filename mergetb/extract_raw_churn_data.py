import os
import re
import json
import sys
import yaml
from extract_raw_data import get_bandwidth_bytes_or_start_time, metrics_to_extract, create_results_dict, simulation_ran_successfully, get_proxy_request, get_incoming_messages, get_histogram_metrics 

def get_metrics_data(data_dir, path_length, n_clients, results, variable, index):
    if metrics_to_extract[0] not in results:
        create_results_dict(results, metrics_to_extract)

    directory = os.path.join(data_dir, variable)
    metrics_file = os.path.join(directory, f"metrics_{index}.txt")
    
    print(f"Getting metrics data from {metrics_file}")
    if os.path.exists(metrics_file):
        with open(metrics_file, 'r') as f:
            content = f.read()
        
        for metric in metrics_to_extract:
            if metric == "loopix_bandwidth_bytes" or metric == "loopix_start_time_seconds":
                get_bandwidth_bytes_or_start_time(results, content, metric, index)

            elif metric == "loopix_number_of_proxy_requests":
                get_proxy_request(results, content, metric, index)

            elif metric == "loopix_incoming_messages":
                get_incoming_messages(results, content, metric, index)

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
        runs = [f for f in os.listdir(run_dir) if not f.endswith(signal_log_suffix) and not f.endswith(json_suffix)]

        print(runs)

        with open(os.path.join(run_dir, f'{variable}.json'), 'r') as f:
            run_keys = json.load(f)

        for run in runs:
            if variable == "retry_duplicates":
                run_key = run
            else:
                run_key = run_keys[run]

            results[variable][run_key] = {}
            node_dir = os.path.join(run_dir, run)
            nodes = os.listdir(node_dir)
            print(nodes)

            for enum_node, node in enumerate(nodes):
                print(node)
                node_data_dir = os.path.join(node_dir, node, "data")
                print(node_data_dir)

                prefix = "metrics_"

                files = os.listdir(node_data_dir)

                metrics_files = [file for file in files if file.startswith(prefix) and os.path.isfile(os.path.join(node_data_dir, file))]

                print(metrics_files)

                for time_index, metrics_file in enumerate(metrics_files):
                    if time_index*20 not in results[variable][run_key]:
                        results[variable][run_key][time_index*20] = {}  

                    print(f"time index: {time_index}")             

                    print(f"Getting metrics data from run {variable} {time_index*20} for node {enum_node}")

                    get_metrics_data(node_data_dir, path_length, n_clients, results[variable][run_key][time_index*20], "", time_index)

                    
        print(results)
    with open(f'{base_path}/raw_metrics.json', 'w') as f:
        json.dump(results, f, indent=2)

if __name__ == "__main__":
    main()