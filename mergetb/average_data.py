import numpy as np
import json
import sys
import os
metrics_to_extract = [
    "loopix_total_bandwidth_mb",
    "loopix_reliability",
    "loopix_incoming_messages",
    "loopix_end_to_end_latency_seconds",
    "loopix_encryption_latency_milliseconds",
    "loopix_client_delay_milliseconds",
    "loopix_decryption_latency_milliseconds",
    "loopix_mixnode_delay_milliseconds",
    "loopix_provider_delay_milliseconds",
]

def get_data(directory):
    with open(os.path.join(directory, 'raw_metrics.json'), 'r') as f:
        data = json.load(f)
    return data

def calculate_average_data(data, duration):
    results = {}
    for metric in metrics_to_extract:
        try:
            if metric == "loopix_total_bandwidth_mb":
                results["loopix_total_bandwidth_mb"] = np.sum(data["loopix_bandwidth_bytes"]) / (1024 * 1024)
            elif metric == "loopix_reliability":
                total_requests = np.array(data["loopix_number_of_proxy_requests"])
                latency_data = np.array(data["loopix_end_to_end_latency_seconds"]["count"])
                latency_data = np.pad(latency_data, (0, len(total_requests) - len(latency_data)), 'constant')
                print(f"latency_data: {latency_data}, total_requests: {total_requests}")
                if np.sum(total_requests) == 0:
                    results[metric] = 0
                else:
                    results[metric] = np.sum(latency_data) / np.sum(total_requests)       
                print(f"results[metric]: {results[metric]}")         
            elif metric == "loopix_incoming_messages":
                results[metric] = np.mean(data[metric]) / (duration - np.mean(data["loopix_start_time_seconds"]))
                results[f"{metric}_std"] = np.std(data[metric]) / (duration - np.mean(data["loopix_start_time_seconds"]))
            else:
                if np.sum(data[metric]["count"]) == 0:
                    results[metric] = 0
                else:
                    results[metric] = np.sum(data[metric]["sum"]) / np.sum(data[metric]["count"])
        except Exception as e:
            print(f"Error for metric {metric}: {e}")
            results[metric] = 0
            results[f"{metric}_std"] = 0
    return results

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: python average_data.py <directory> <duration>")
        sys.exit(1)

    directory = sys.argv[1]
    duration = float(sys.argv[2])
    data = get_data(directory)

    average_data = {}

    print(f"variables: {data.keys()}")

    for variable, runs in data.items():
        print(f"runs: {runs.keys()}")
        for run, runs in runs.items():

            if variable not in average_data:
                average_data[variable] = {run: {}}
            else:
                average_data[variable][run] = {}

            print("Getting data for: ", variable, run)
            
            average_data[variable][run] = calculate_average_data(runs, duration)

    with open(os.path.join(directory, 'average_data.json'), 'w') as f:
        json.dump(average_data, f, indent=2)

   

        
