import matplotlib.pyplot as plt
import numpy as np
import json
import sys
import os

def get_data(directory):
    with open(os.path.join(directory, 'raw_average_data.json'), 'r') as f:
        return json.load(f)

def convert_to_milliseconds(seconds):
    return seconds * 1000

def save_plot_directory(directory):
    os.makedirs(directory, exist_ok=True)

def plot_latency_components(directory, path_length, variable, run):
    indices = list(run.keys())
    n_runs = len(indices)

    n_mixnode_delay = (path_length + 1) * 2
    n_decryption_delay = (path_length + 3) * 2
    n_network_delay = (path_length - 1 ) * 2 + 8
    provider_delay = np.array([run[i]["loopix_provider_delay_milliseconds"] * 2 for i in indices])
    mixnode_delay = np.array([run[i]["loopix_mixnode_delay_milliseconds"] * n_mixnode_delay for i in indices])
    decryption_delay = np.array([run[i]["loopix_decryption_latency_milliseconds"] * n_decryption_delay for i in indices])
    client_delay = np.array([run[i]["loopix_client_delay_milliseconds"] * 2 for i in indices])
    encryption_delay = np.array([run[i]["loopix_encryption_latency_milliseconds"] * 2 for i in indices])
    network_delay = np.full_like(provider_delay, 15 * n_network_delay)

    total_latency = [convert_to_milliseconds(run[i]["loopix_end_to_end_latency_seconds"]) for i in indices]

    bar_width = 0.6
    x = np.arange(n_runs)

    plt.bar(x, encryption_delay, width=bar_width, label="Encryption Delay", bottom=0)
    plt.bar(x, client_delay, width=bar_width, label="Client Delay", bottom=encryption_delay)
    plt.bar(x, decryption_delay, width=bar_width, label="Decryption Delay", bottom=encryption_delay + client_delay)
    plt.bar(x, mixnode_delay, width=bar_width, label="Mixnode Delay", bottom=encryption_delay + client_delay + decryption_delay)
    plt.bar(x, provider_delay, width=bar_width, label="Provider Delay", bottom=encryption_delay + client_delay + decryption_delay + mixnode_delay)
    plt.bar(x, network_delay, width=bar_width, label="Network Delay", bottom=encryption_delay + client_delay + decryption_delay + mixnode_delay + provider_delay)

    plt.bar(x, total_latency, width=bar_width, color='none', edgecolor='red', linestyle='--', linewidth=2, label="End-to-End Latency")

    plt.xticks(x, indices)
    plt.xlabel("Run Index")
    plt.ylabel("Latency (milliseconds)")
    plt.title(f"Stacked Latency Components for {variable}")
    plt.legend()
    plt.tight_layout()

    save_plot_directory(f"plots/{variable}")
    plt.savefig(f"plots/{variable}/latency.png")
    plt.clf()

def plot_reliability(directory, variable, run):
    indices = list(run.keys())
    reliability = [run[i]["loopix_reliability"] for i in indices]

    plt.plot(indices, reliability, marker='o', linestyle='-', color='blue', label='Reliability')
    plt.xlabel("Run Index")
    plt.ylabel("Reliability (%)")
    plt.ylim(0, 1)
    plt.title(f"Reliability for {variable}")
    plt.legend()
    plt.tight_layout()

    save_plot_directory(f"{directory}/plots/{variable}")
    plt.savefig(f"{directory}/plots/{variable}/reliability.png")
    plt.clf()

def plot_bandwidth(directory, variable, run):
    indices = list(run.keys())
    bandwidth = [run[i]["loopix_total_bandwidth_mb"] for i in indices]

    plt.plot(indices, bandwidth, marker='o', linestyle='-', color='green', label='Bandwidth')
    plt.xlabel("Run Index")
    plt.ylabel("Bandwidth (Bytes/Second)")
    plt.ylim(0, max(bandwidth) * 1.1)
    plt.title(f"Bandwidth for {variable}")
    plt.legend()
    plt.tight_layout()

    save_plot_directory(f"{directory}/plots/{variable}")
    plt.savefig(f"{directory}/plots/{variable}/bandwidth.png")
    plt.clf()

if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("Usage: python plot_data.py <directory> <path_length>")
        sys.exit(1)

    directory = sys.argv[1]
    path_length = int(sys.argv[2])
    data = get_data(directory)

    for variable, run in data.items():
        plot_latency_components(directory, path_length, variable, run)
        plot_reliability(directory, variable, run)
        plot_bandwidth(directory, variable, run)