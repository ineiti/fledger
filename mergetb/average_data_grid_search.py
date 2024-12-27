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

def calculate_average_data(data):
    results = {}
    for metric in metrics_to_extract:
        try:
            if metric == "loopix_total_bandwidth_mb":
                results["loopix_total_bandwidth_mb"] = np.sum(data["loopix_bandwidth_bytes"]) / (1024 * 1024)
            elif metric == "loopix_reliability":
                successful_requests = np.sum(data["loopix_end_to_end_latency_seconds"]["count"])
                total_requests = np.sum(np.array(data["loopix_number_of_proxy_requests"]))
                print(f"successful_requests: {successful_requests}, total_requests: {total_requests}")
                print(f"successful_requests / total_requests: {successful_requests / total_requests}")
                results[metric] = successful_requests / total_requests
            elif metric == "loopix_incoming_messages":
                results[metric] = np.mean(data[metric])
            else:
                if len(data[metric]["count"]) == 0:
                    results[metric] = 0
                else:
                    results[metric] = np.sum(data[metric]["sum"]) / np.sum(data[metric]["count"])
        except Exception as e:
            print(f"Error for metric {metric}: {e}")
            results[metric] = 0
    return results

if __name__ == "__main__":
    if len(sys.argv) < 1:
        print("Usage: python average_data.py <directory>")
        sys.exit(1)

    directory = sys.argv[1]
    data = get_data(directory)

    average_data = {}

    print(f"time_pulls: {data.keys()}")

    for time_pull, max_retrieves in data.items():
        print(f"max_retrieves: {max_retrieves.keys()}")
        for max_retrieve, runs in max_retrieves.items():

            if time_pull not in average_data:
                average_data[time_pull] = {max_retrieve: {}}
            else:
                average_data[time_pull][max_retrieve] = {}

            print("Getting data for: ", time_pull, max_retrieve)
            # print(runs.keys())
            average_data[time_pull][max_retrieve] = calculate_average_data(runs)

    with open(os.path.join(directory, 'raw_average_data.json'), 'w') as f:
        json.dump(average_data, f, indent=2)

   

        
