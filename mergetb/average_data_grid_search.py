import numpy as np
import json
import sys
import os
from average_data import calculate_average_data, metrics_to_extract

def get_data(directory):
    with open(os.path.join(directory, 'raw_metrics.json'), 'r') as f:
        data = json.load(f)
    return data

if __name__ == "__main__":
    if len(sys.argv) < 1:
        print("Usage: python average_data.py <directory> <duration>")
        sys.exit(1)

    directory = sys.argv[1]
    duration = float(sys.argv[2])
    data = get_data(directory)


    average_data_time_pull = {}
    average_data_max_retrieve = {str(i): {} for i in range(1, 13)}

    print(f"time_pulls: {data.keys()}")

    for time_pull, max_retrieves in data.items():
        print(f"max_retrieves: {max_retrieves.keys()}")
        for max_retrieve, runs in max_retrieves.items():

            if time_pull not in average_data_time_pull:
                average_data_time_pull[time_pull] = {max_retrieve: {}}
            else:
                average_data_time_pull[time_pull][max_retrieve] = {}

            average_data_max_retrieve[max_retrieve][time_pull] = runs

            print("Getting data for: ", time_pull, max_retrieve)
            average_data_time_pull[time_pull][max_retrieve] = calculate_average_data(runs, duration)
            average_data_max_retrieve[max_retrieve][time_pull] = average_data_time_pull[time_pull][max_retrieve]

    average_data = {"time_pulls": average_data_time_pull, "max_retrieves": average_data_max_retrieve}
    with open(os.path.join(directory, 'average_data.json'), 'w') as f:
        json.dump(average_data, f, indent=2)

   

        
