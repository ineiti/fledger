import numpy as np
import json
import sys
import os
from average_data import metrics_to_extract, get_data, calculate_average_data
import extract_raw_data

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
        average_data[variable] = {}
        print(f"runs: {runs.keys()}")
        for run, runs in runs.items():
            average_data[variable][run] = {}
            print("Getting data for: ", variable, run)

            for time_index, time_data in runs.items():
                if time_index != "0":
                    prev_step = runs[str(int(time_index) - 20)]

                    for metric in extract_raw_data.metrics_to_extract:
                        if metric in ["loopix_bandwidth_bytes", "loopix_incoming_messages", "loopix_number_of_proxy_requests"]:
                            data[metric] = [t - p for t, p in zip(time_data[metric], prev_step[metric])]
                        elif metric == "loopix_start_time_seconds":
                            data[metric] = time_data[metric]
                        else:
                            data[metric]["sum"] = [t - p for t, p in zip(time_data[metric]["sum"], prev_step[metric]["sum"])]
                            data[metric]["count"] = [t - p for t, p in zip(time_data[metric]["count"], prev_step[metric]["count"])]
                else:
                    data = time_data
                average_data[variable][run][time_index] = {}

                print(data)
                average_data[variable][run][time_index] = calculate_average_data(data, duration)

    with open(os.path.join(directory, 'average_data.json'), 'w') as f:
        json.dump(average_data, f, indent=2)

   

        
