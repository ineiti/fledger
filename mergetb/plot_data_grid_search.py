import matplotlib.pyplot as plt
import numpy as np
import json
import sys
import os

from plot_data import *

y_labels = range(1, 13)
xlabel = "Time-to-Pull"
ylabel = "Number of messages retrieved"

def plot_heatmap_bandwidth(directory, data):

    x_labels = data.keys()
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
    plt.xlabel(xlabel)
    plt.ylabel(ylabel)

    plt.title('Heatmap of Total Bandwidth')
    plt.savefig(f"{directory}/heatmap_bandwidth.png")
    plt.clf()

def plot_heatmap_latency(directory, data):

    x_labels = data.keys()

    Z = np.zeros((len(y_labels), len(x_labels)))
    
    for i, x in enumerate(x_labels):
        for j, y in enumerate(y_labels):
            Z[j, i] = convert_to_milliseconds(data.get(str(x), {}).get(str(y), {}).get("loopix_end_to_end_latency_seconds", 0))

    plt.figure(figsize=(10, 8))

    threshold = convert_to_milliseconds(10)
    masked_Z = np.ma.masked_greater(Z, threshold)
    threshold = convert_to_milliseconds(1)
    masked_Z = np.ma.masked_less(Z, threshold)

    heatmap = plt.imshow(masked_Z, aspect='auto', cmap='viridis', origin='lower')

    plt.colorbar(heatmap, label='Latency (milliseconds)')

    plt.xticks(ticks=np.arange(len(x_labels)), labels=x_labels)
    plt.yticks(ticks=np.arange(len(y_labels)), labels=y_labels)
    plt.xlabel(xlabel)
    plt.ylabel(ylabel)

    plt.title('Heatmap of End-to-End Latency')
    plt.savefig(f"{directory}/heatmap_latency.png")
    plt.clf()

def plot_average_reliability(directory, data):

    plt.figure(figsize=(20, 6))
    x_labels = data.keys()
    Z = np.zeros((len(y_labels), len(x_labels)))

    average_reliability = []
    for time_pull, max_retrieves in data.items():
        reliability_per_time_pull = []
        for max_retrieve, data in max_retrieves.items():
            print(f"keys: {data.keys()}")

            reliability = data["loopix_reliability"]
            if reliability != 0:
                reliability_per_time_pull.append(reliability*100)

        average_reliability.append(np.mean(reliability_per_time_pull))

    plt.plot(x_labels, average_reliability, marker='o', linestyle='-', color='blue', label='Reliability')
    plt.xlabel(xlabel)
    plt.ylabel("Reliability (%)")
    plt.ylim(0, 100)
    plt.title("Percentage of Successful Web Proxy Requests")
    plt.legend()
    plt.tight_layout()
    plt.savefig(f"{directory}/average_reliability.png")
    plt.clf()

def plot_average_latency_max_retrieve(directory, data):

    fig, ax1 = plt.subplots(figsize=(20, 6))
    x_labels = data.keys()

    average_latency = []
    average_bandwidth = []
    for max_retrieve, time_pulls in data.items():
        latency_per_max_retrieve = []
        bandwidth_per_max_retrieve = []
        for time_pull, data in time_pulls.items():

            latency = data.get("loopix_end_to_end_latency_seconds", 0)
            latency_per_max_retrieve.append(convert_to_milliseconds(latency))
                
            bandwidth = data.get("loopix_total_bandwidth_mb", 0)
            bandwidth_per_max_retrieve.append(bandwidth)

        average_latency.append(np.mean(latency_per_max_retrieve))
        average_bandwidth.append(np.mean(bandwidth_per_max_retrieve))

    ax1.plot(x_labels, average_latency, marker='o', linestyle='-', color='blue', label='Latency')
    ax1.set_xlabel(ylabel)
    ax1.set_ylabel("Latency (milliseconds)", color='blue')
    ax1.tick_params(axis='y', labelcolor='blue')

    ax2 = ax1.twinx()
    ax2.plot(x_labels, average_bandwidth, marker='x', linestyle='--', color='orange', label='Bandwidth')
    ax2.set_ylabel("Bandwidth (MB)", color='orange')
    ax2.tick_params(axis='y', labelcolor='orange')

    ax1.set_ylim(0, max(average_latency) * 1.1)
    ax2.set_ylim(0, max(average_bandwidth) * 1.1)
    plt.title("Average Latency and Bandwidth")
    fig.tight_layout()
    plt.savefig(f"{directory}/average_latency_max_retrieve.png")
    plt.clf()

def plot_average_latency_time_pull(directory, data):

    fig, ax1 = plt.subplots(figsize=(20, 6))
    x_labels = data.keys()

    average_latency = []
    average_bandwidth = []
    for time_pull, max_retrieves in data.items():
        latency_per_time_pull = []
        bandwidth_per_time_pull = []
        for max_retrieve, data in max_retrieves.items():
        
            latency = data.get("loopix_end_to_end_latency_seconds", 0)
            latency_per_time_pull.append(convert_to_milliseconds(latency))
                
            bandwidth = data.get("loopix_total_bandwidth_mb", 0)
            bandwidth_per_time_pull.append(bandwidth)

        average_latency.append(np.mean(latency_per_time_pull))
        average_bandwidth.append(np.mean(bandwidth_per_time_pull))

    ax1.plot(x_labels, average_latency, marker='o', linestyle='-', color='blue', label='Latency')
    ax1.set_xlabel(xlabel)
    ax1.set_ylabel("Latency (milliseconds)", color='blue')
    ax1.tick_params(axis='y', labelcolor='blue')

    ax2 = ax1.twinx() 
    ax2.plot(x_labels, average_bandwidth, marker='x', linestyle='--', color='orange', label='Bandwidth')
    ax2.set_ylabel("Bandwidth (MB)", color='orange')
    ax2.tick_params(axis='y', labelcolor='orange')

    ax1.set_ylim(0, max(average_latency) * 1.1)
    ax2.set_ylim(0, max(average_bandwidth) * 1.1)
    plt.title("Average Latency and Bandwidth")
    fig.tight_layout()
    plt.savefig(f"{directory}/average_latency_time_pull.png")
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

    max_retrieves = {i: [] for i in range(0, 13)}

    time_pulls = data["time_pulls"]
    for variable, run in time_pulls.items():
        print(f"Plotting data for time pull {variable}")
        print(run.keys())
        plot_dir = os.path.join(plots_dir, "time_pull", str(variable))
        os.makedirs(plot_dir, exist_ok=True)
        plot_latency_components(plot_dir, path_length, variable, run)
        plot_reliability(plot_dir, variable, run)
        plot_bandwidth(plot_dir, duration, variable, run)
        plot_incoming_messages(plot_dir, variable, run)
        plot_latency_and_bandwidth(plot_dir, variable, run)

    max_retrieves = data["max_retrieves"]
    for max_retrieve, run in max_retrieves.items():
        print(f"Plotting data for max retrieve {max_retrieve}")
        plot_dir = os.path.join(plots_dir, "max_retrieve", str(max_retrieve))
        os.makedirs(plot_dir, exist_ok=True)
        plot_latency_components(plot_dir, path_length, max_retrieve, run)
        plot_reliability(plot_dir, max_retrieve, run)
        plot_bandwidth(plot_dir, duration, max_retrieve, run)
        plot_incoming_messages(plot_dir, max_retrieve, run)
        plot_latency_and_bandwidth(plot_dir, max_retrieve, run)
    
    plot_average_latency_max_retrieve(plots_dir, max_retrieves)
    plot_average_latency_time_pull(plots_dir, time_pulls)
    plot_heatmap_bandwidth(plots_dir, time_pulls)
    plot_heatmap_latency(plots_dir, time_pulls)
    plot_average_reliability(plots_dir, time_pulls)
