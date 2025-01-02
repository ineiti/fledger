import os
import re
import json
import sys
import yaml
from extract_raw_data import get_bandwidth_bytes_or_start_time, get_proxy_request, get_incoming_messages, get_histogram_metrics, 

def get_metrics_data(data_dir, path_length, n_clients, results, variable, index):

    if not simulation_ran_successfully(data_dir, variable, index):
        print(f"Skipping run {variable} {index}, no end-to-end latency data found")
        return False

    create_results_dict(results, metrics_to_extract)

    for i in range(path_length*(path_length - 1) + path_length + n_clients):

        print(f"Getting metrics data for node-{i}")
        dir = f"{data_dir}/{variable}"
        metrics_file = os.path.join(dir, f"metrics_{index}_node-{i}.txt")
        
        if os.path.exists(metrics_file):
            with open(metrics_file, 'r') as f:
                content = f.read()
            
            for metric in metrics_to_extract:
                if metric == "loopix_bandwidth_bytes" or metric == "loopix_start_time_seconds":
                    get_bandwidth_bytes_or_start_time(results, content, metric, i)

                elif metric == "loopix_number_of_proxy_requests":
                    get_proxy_request(results, content, metric, i)

                elif metric == "loopix_incoming_messages":
                    get_incoming_messages(results, content, metric, i)

                else:
                    get_histogram_metrics(results, content, metric, i)

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
        run_dir = os.path.join(data_dir, variable)
        runs = [f for f in os.listdir(run_dir) if not f.endswith(signal_log_suffix) and not f.endswith(json_suffix)]

        print(runs)

        for run in runs:
            node_dir = os.path.join(run_dir, run)
            nodes = os.listdir(node_dir)
            print(nodes)
            for node in nodes:
                node_data_dir = os.path.join(node_dir, node, "data")
                print(node_data_dir)

                prefix = "metrics_"

                files = os.listdir(node_data_dir)

                metrics_files = [file for file in files if file.startswith(prefix) and os.path.isfile(os.path.join(node_data_dir, file))]

                print(metrics_files)

                for index in range(runs):
                    with open(os.path.join(directory, f'{index}_config.yaml'), 'r') as f:
                        config = yaml.safe_load(f)

                    print(variable == 'control')

                    if variable == 'control':
                        run_value = index
                    else:
                        try:
                            run_value = config[variable]
                        except:
                            continue

                    if variable not in results.keys():
                        results[variable] = {str(run_value): {}}
                    else:
                        results[variable][str(run_value)] = {}

                    print(f"Getting metrics data from run {variable} {index}")

                    get_metrics_data(data_dir, path_length, n_clients, results[variable][str(run_value)], variable, index)

                    
        print(results)
    with open(f'{base_path}/raw_metrics.json', 'w') as f:
        json.dump(results, f, indent=2)

if __name__ == "__main__":
    main()