import matplotlib.pyplot as plt
import numpy as np
import json
import sys
import os

from plot_data import *

def plot_heatmap_bandwidth(directory, data):

    x_labels = data.keys()
    y_labels = range(1, 13)
    Z = np.zeros((len(y_labels), len(x_labels)))
    
    for i, x in enumerate(x_labels):
        for j, y in enumerate(y_labels):
            Z[j, i] = data.get(str(x), {}).get(str(y), {}).get("loopix_total_bandwidth_mb", 0)

    plt.figure(figsize=(10, 8))

    threshold = 500
    masked_Z = np.ma.masked_less(Z, threshold)
    heatmap = plt.imshow(masked_Z, aspect='auto', cmap='viridis', origin='lower')

    plt.colorbar(heatmap, label='Total Bandwidth (MB)')

    plt.xticks(ticks=np.arange(len(x_labels)), labels=x_labels)
    plt.yticks(ticks=np.arange(len(y_labels)), labels=y_labels)
    plt.xlabel('Time Pull')
    plt.ylabel('Max Retrieve')

    plt.title('Heatmap of Total Bandwidth')
    plt.savefig(f"{directory}/heatmap_bandwidth.png")
    plt.clf()

def plot_heatmap_latency(directory, data):

    x_labels = data.keys()
    y_labels = range(1, 13)

    Z = np.zeros((len(y_labels), len(x_labels)))
    
    for i, x in enumerate(x_labels):
        for j, y in enumerate(y_labels):
            Z[j, i] = data.get(str(x), {}).get(str(y), {}).get("loopix_end_to_end_latency_seconds", 0)

    plt.figure(figsize=(10, 8))

    threshold = 10
    masked_Z = np.ma.masked_greater(Z, threshold)
    threshold = 1
    masked_Z = np.ma.masked_less(Z, threshold)

    heatmap = plt.imshow(masked_Z, aspect='auto', cmap='viridis', origin='lower')

    plt.colorbar(heatmap, label='Latency (seconds)')

    plt.xticks(ticks=np.arange(len(x_labels)), labels=x_labels)
    plt.yticks(ticks=np.arange(len(y_labels)), labels=y_labels)
    plt.xlabel('Time Pull')
    plt.ylabel('Max Retrieve')

    plt.title('Heatmap of End-to-End Latency')
    plt.savefig(f"{directory}/heatmap_latency.png")
    plt.clf()

def plot_average_reliability(directory, data):

    x_labels = data.keys()
    y_labels = range(1, 13)
    Z = np.zeros((len(y_labels), len(x_labels)))

    average_reliability = []
    for time_pull, max_retrieves in data.items():
        reliability_per_time_pull = []
        for max_retrieve, data in max_retrieves.items():
            reliability = data["loopix_reliability"]
            if reliability != 0:
                reliability_per_time_pull.append(reliability*100)

        average_reliability.append(np.mean(reliability_per_time_pull))

    plt.plot(x_labels, average_reliability, marker='o', linestyle='-', color='blue', label='Reliability')
    plt.xlabel("Time Pull")
    plt.ylabel("Reliability (%)")
    plt.ylim(0, 100)
    plt.title("Average Reliability")
    plt.legend()
    plt.tight_layout()
    plt.savefig(f"{directory}/average_reliability.png")
    plt.clf()


if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("Usage: python plot_data.py <data_dir> <path_length> <duration>")
        sys.exit(1)

    data_dir = sys.argv[1]
    path_length = int(sys.argv[2])
    duration = float(sys.argv[3])
    data = get_data(data_dir)
    
    plots_dir = os.path.join(data_dir, "plots")

    for variable, run in data.items():
        print(f"Plotting data for {variable}")
        print(run.keys())
        plot_latency_components(plots_dir, path_length, variable, run)
        plot_reliability(plots_dir, variable, run)
        plot_bandwidth(plots_dir, duration, variable, run)
        plot_incoming_messages(plots_dir, variable, run)

    plot_heatmap_bandwidth(plots_dir, data)
    plot_heatmap_latency(plots_dir, data)
    plot_average_reliability(plots_dir, data)
