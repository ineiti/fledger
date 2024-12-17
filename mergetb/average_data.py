import numpy as np
import json
import sys

metrics_to_extract = [
    "loopix_bandwidth_bytes_per_second",
    "loopix_reliability",
    "loopix_end_to_end_latency_seconds",
    "loopix_encryption_latency_milliseconds",
    "loopix_client_delay_milliseconds",
    "loopix_decryption_latency_milliseconds",
    "loopix_mixnode_delay_milliseconds",
    "loopix_provider_delay_milliseconds",
]

def get_data():
    with open('raw_metrics.json', 'r') as f:
        data = json.load(f)
    return data

def calculate_average_data(duration, data):
    results = {}
    for metric in metrics_to_extract:
        if metric == "loopix_bandwidth_bytes_per_second":
            results[metric] = np.sum(data["loopix_bandwidth_bytes"])/(duration - data["loopix_start_time_seconds"][0])
        elif metric == "loopix_reliability":
            successful_requests = data["loopix_end_to_end_latency_seconds"]["count"]
            results[metric] = np.mean(np.array(successful_requests)/np.array(data["loopix_number_of_proxy_requests"]))
        else:
            results[metric] = np.sum(data[metric]["sum"]) / np.sum(data[metric]["count"])
    return results

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python average_data.py <duration>")
        sys.exit(1)

    duration = int(sys.argv[1])
    data = get_data()

    average_data = {}

    for variable_name, runs in data.items():
        average_data[variable_name] = {}

        for index, metrics_data in runs.items():
            print(f"Calculating average data for {variable_name} {index}")
            average_data[variable_name][index] = calculate_average_data(duration, metrics_data)

    with open('average_data.json', 'w') as f:
        json.dump(average_data, f, indent=2)

   

        
