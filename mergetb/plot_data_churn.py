import matplotlib.pyplot as plt
import numpy as np
import json
import sys
import os
from plot_data import get_data, save_plot_directory, plot_latency_components, plot_reliability, plot_incoming_messages, plot_latency, plot_bandwidth, plot_latency_and_bandwidth, plot_reliability_latency
from matplotlib.ticker import FuncFormatter

def log_format(y, _):
    return f"{y:.0f}"
    
x_axis_name = {"lambda_loop": "Loop and Drop Messages per Second", "lambda_payload": "Payload Messages per Second from the Client"}

if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("Usage: python plot_data.py <directory> <path_length> <duration>")
        sys.exit(1)


    directory = sys.argv[1]
    path_length = int(sys.argv[2])
    duration = int(sys.argv[3])
    data = get_data(directory)

    retry_levels = data["retry"].keys()
    points_in_time = list(data["retry"]["0"].keys())
    points_in_time_labels = [
        "All Nodes Live", "1 Node Down", "2 Nodes Down", "3 Nodes Down"
    ]

    reliability_data = {
        retry: [
            data["retry"][retry][str(pt)]["loopix_reliability"] 
            for pt in points_in_time
        ]
        for retry in retry_levels
    }

    plt.figure(figsize=(12, 8))

    bar_width = 0.2
    x_indexes = range(len(points_in_time_labels))
    offset = 0


    plt.yscale('log')
    plt.gca().yaxis.set_major_formatter(FuncFormatter(log_format))

    for retry, reliabilities in reliability_data.items():
        reliability = [r*100 for r in reliabilities]
        plt.bar(
            [x + offset for x in x_indexes],
            reliability,
            width=bar_width,
            label=f"{retry} Retry",
        )
        offset += bar_width

    # Customizing the plot
    plt.title("Reliability vs Node Status", fontsize=16)
    plt.xlabel("Node Status", fontsize=14)
    plt.ylabel("Reliability", fontsize=14)
    plt.xticks([x + bar_width * (len(retry_levels) - 1) / 2 for x in x_indexes], points_in_time_labels, fontsize=12)
    plt.legend(fontsize=12)
    plt.grid(axis='y', linestyle='--', alpha=0.7)

    # Save and close the plot
    save_plot_directory(directory)
    plt.savefig(os.path.join(directory, "reliability_churn_bar_chart.png"))
    plt.clf()
    plt.close()



