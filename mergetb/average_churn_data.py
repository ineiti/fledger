import numpy as np
import json
import sys
import os
from average_data import metrics_to_extract, get_data, calculate_average_data
import extract_raw_data 


if __name__ == "__main__":
    if len(sys.argv) < 1:
        print("Usage: python average_data.py <directory> <duration>")
        sys.exit(1)

    directory = sys.argv[1]
    duration = float(sys.argv[2])
    data = get_data(directory)
    
    average_data = {}

    for variable, runs in data.items():
        average_data[variable] = {}
        for try_index, kill_runs in runs.items():
            average_data[variable][try_index] = {}
            for kill, runs in kill_runs.items():
                print(runs.keys())
                print(f"try_index: {try_index}")
                print(f"run: {kill}")

                average_data[variable][try_index][kill] = calculate_average_data(runs, duration)

    with open(os.path.join(directory, 'average_data.json'), 'w') as f:
        json.dump(average_data, f, indent=2)


        
