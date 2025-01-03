import matplotlib.pyplot as plt
import numpy as np
import json
import sys
import os
from plot_data import get_data, plot_latency_components, plot_reliability, plot_incoming_messages, plot_latency, plot_bandwidth, plot_latency_and_bandwidth, plot_reliability_latency

x_axis_name = {"lambda_loop": "Loop and Drop Messages per Second", "lambda_payload": "Payload Messages per Second from the Client"}

if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("Usage: python plot_data.py <directory> <path_length> <duration>")
        sys.exit(1)


    directory = sys.argv[1]
    path_length = int(sys.argv[2])
    duration = int(sys.argv[3])
    data = get_data(directory)

    for variable, run in data.items():
        print(f"Plotting {variable}")
        for run_index, run_data in run.items():
            plot_dir = os.path.join(directory, "plots", variable, run_index)
            print(f"Plotting {run_index} with plot_dir {plot_dir}")
            plot_latency_components(plot_dir, path_length, variable, run_data)
            plot_reliability(plot_dir, variable, run_data)
            plot_incoming_messages(plot_dir, variable, run_data)
            plot_latency(plot_dir, variable, run_data)
            plot_bandwidth(plot_dir, duration, variable, run_data)
            plot_latency_and_bandwidth(plot_dir, variable, run_data)
            # plot_reliability_incoming_latency(directory, variable, run)
            plot_reliability_latency(plot_dir, variable, run_data)
