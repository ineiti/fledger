import os
import re
import json
import sys


metrics_to_extract = [
    "loopix_bandwidth_bytes",
    "loopix_number_of_proxy_requests",
    "loopix_start_time_seconds",
    "loopix_incoming_messages"
    "loopix_end_to_end_latency_seconds",
    "loopix_encryption_latency_milliseconds",
    "loopix_client_delay_milliseconds",
    "loopix_decryption_latency_milliseconds",
    "loopix_mixnode_delay_milliseconds",
    "loopix_provider_delay_milliseconds",
]

def simulation_ran_successfully(data_dir, variable, index):
    dir = f"{data_dir}/{variable}"
    metrics_file = os.path.join(dir, f"metrics_{index}_node-1.txt")
    # print(metrics_file)
    # print(os.path.exists(metrics_file))
    # print(os.listdir(dir))
    if not os.path.exists(metrics_file):
        return False

    with open(metrics_file, 'r') as f:
        content = f.read()

    if "loopix_end_to_end_latency_seconds" in content:
        return True
    else:
        return False


def get_metrics_data(data_dir, path_length, results, variable, index):

    if not simulation_ran_successfully(data_dir, variable, index):
        print(f"Skipping run {variable} {index}, no end-to-end latency data found")
        return False

    for metric in metrics_to_extract:
        if metric == "loopix_bandwidth_bytes" or metric == "loopix_number_of_proxy_requests" or metric == "loopix_start_time_seconds":
            results[metric] = []
        else:
            results[metric] = {"sum": [], "count": []}

    for i in range(path_length*path_length + path_length * 2):
        dir = f"{data_dir}/{variable}"
        metrics_file = os.path.join(dir, f"metrics_{index}_node-{i}.txt")
        
        if os.path.exists(metrics_file):
            with open(metrics_file, 'r') as f:
                content = f.read()
            
            for metric in metrics_to_extract:
                if metric == "loopix_bandwidth_bytes":
                    pattern = rf"{metric}\s+([0-9.e+-]+)$"
                    match = re.search(pattern, content, re.MULTILINE)
                    if match:
                        results[metric].append(float(match.group(1)))
                    else:
                        print(f"Error for node-{i}: match {match}")

                elif metric == "loopix_number_of_proxy_requests" or metric == "loopix_start_time_seconds":
                    pattern = rf"{metric}\s+([0-9.e+-]+)$"
                    match = re.search(pattern, content, re.MULTILINE)
                    if match:
                        results[metric].append(float(match.group(1)))
                elif metric == "loopix_incoming_messages":
                    pattern = rf"{metric}\s+([0-9.e+-]+)$"
                    match = re.search(pattern, content, re.MULTILINE)
                    provider_pattern = "loopix_provider_delay_milliseconds"
                    client_pattern = "loopix_number_of_proxy_requests"
                    if not provider_pattern in content and not client_pattern in content:
                        results[metric].append(float(match.group(1)))
                else:
                    pattern_sum = rf"^{metric}_sum\s+([0-9.e+-]+)"
                    pattern_count = rf"^{metric}_count\s+([0-9.e+-]+)"
                    match_sum = re.search(pattern_sum, content, re.MULTILINE)
                    match_count = re.search(pattern_count, content, re.MULTILINE)
                    if match_count and match_sum:
                        results[metric][f"sum"].append(float(match_sum.group(1)))
                        results[metric][f"count"].append(float(match_count.group(1)))
                    elif match_count or match_sum:
                        print(f"Error for node-{i}: match_sum {match_sum}, match_count {match_count}")

    return True
          
def main():
    if len(sys.argv) != 3:
        print("Usage: python extract_raw_data.py <data_dir> <path_length>")
        sys.exit(1)

    data_dir = sys.argv[1]
    path_length = int(sys.argv[2])

    base_path = data_dir
    print(base_path)
    print(os.listdir(base_path))

    results = {}
    variables = [d for d in os.listdir(base_path) if os.path.isdir(os.path.join(base_path, d))]
    print("AAAAAAAAAAAAAA")
    print(results)
    for variable in variables:

        directory = f"{base_path}/{variable}"
        suffix = "_node-0.txt"

        try:
            files = os.listdir(directory)
        except:
            print(f"Skipping {variable}")
            continue

        try:
            runs = sum(1 for file in files
                        if file.endswith(suffix) and os.path.isfile(os.path.join(directory, file)))
            print(runs)

            with open(os.path.join(directory, f'{variable}.json'), 'r') as f:
                values = json.load(f)
            results[variable] = {values[str(index)]: {} for index in range(runs)}

        except:
            if files:   
                runs = sum(1 for file in files
                            if file.endswith(suffix) and os.path.isfile(os.path.join(directory, file)))

                results[variable] = {index: {} for index in range(runs)}
        

        if runs > 0:
            indices_to_remove = []
            for index, value in enumerate(results[variable].keys()):
                print(f"Getting metrics data from run {variable} {value}")
                if not get_metrics_data(data_dir, path_length, results[variable][value], variable, index):
                    indices_to_remove.append(value)
                    
            print(indices_to_remove)
            for index in indices_to_remove:
                results[variable].pop(index, None)

            print(results[variable].keys())

    with open(f'{data_dir}/raw_metrics.json', 'w') as f:
        json.dump(results, f, indent=2)

if __name__ == "__main__":
    main()