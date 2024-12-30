import matplotlib.pyplot as plt
import numpy as np
import json
import sys
import os

def get_data(directory):
    with open(os.path.join(directory, 'average_data.json'), 'r') as f:
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
    plt.figure(figsize=(20, 6))

    plt.bar(x, encryption_delay, width=bar_width, label="Encryption Delay", bottom=0)
    plt.bar(x, client_delay, width=bar_width, label="Client Delay", bottom=encryption_delay)
    plt.bar(x, decryption_delay, width=bar_width, label="Decryption Delay", bottom=encryption_delay + client_delay)
    plt.bar(x, mixnode_delay, width=bar_width, label="Mixnode Delay", bottom=encryption_delay + client_delay + decryption_delay)
    plt.bar(x, provider_delay, width=bar_width, label="Provider Delay", bottom=encryption_delay + client_delay + decryption_delay + mixnode_delay)
    plt.bar(x, network_delay, width=bar_width, label="Network Delay", bottom=encryption_delay + client_delay + decryption_delay + mixnode_delay + provider_delay)

    plt.bar(x, total_latency, width=bar_width, color='none', edgecolor='red', linestyle='--', linewidth=2, label="End-to-End Latency")

    plt.xticks(x, indices)
    plt.xlabel(variable)
    plt.ylabel("Latency (milliseconds)")
    plt.title(f"End-to-End Latency with different components")
    plt.legend()
    plt.tight_layout()

    save_plot_directory(f"{directory}/plots/{variable}")
    plt.savefig(f"{directory}/plots/{variable}/latency.png")
    plt.clf()

def plot_reliability(directory, variable, run):
    indices = list(run.keys())
    reliability = [run[i]["loopix_reliability"]*100 for i in indices]
    reliability_std = [run[i]["loopix_reliability_std"]*100 for i in indices]

    plt.plot(indices, reliability, marker='o', linestyle='-', color='blue', label='Reliability')
    plt.errorbar(indices, reliability, yerr=reliability_std, fmt='o', color='blue')
    plt.xlabel(variable)
    plt.ylabel("Reliability (%)")
    plt.title(f"Percentage of Successful Web Proxy Requests")
    plt.ylim(0, 100)
    plt.legend()
    plt.tight_layout()

    save_plot_directory(f"{directory}/plots/{variable}")
    plt.savefig(f"{directory}/plots/{variable}/reliability.png")
    plt.clf()

def plot_incoming_messages(directory, variable, run):
    indices = list(run.keys())
    incoming_messages = [run[i]["loopix_incoming_messages"] for i in indices]
    incoming_messages_std = [run[i]["loopix_incoming_messages_std"] for i in indices]
    
    plt.plot(indices, incoming_messages, marker='o', linestyle='-', color='blue', label='Incoming Messages')
    plt.errorbar(indices, incoming_messages, yerr=incoming_messages_std, fmt='o', color='blue')
    plt.xlabel(variable)
    plt.ylabel("Incoming Messages")
    plt.title(f"Number of Incoming Messages per Second per Mixnode")
    plt.legend()
    plt.tight_layout()

    save_plot_directory(f"{directory}/plots/{variable}")
    plt.savefig(f"{directory}/plots/{variable}/incoming_messages.png")
    plt.clf()

def plot_bandwidth(directory, duration, variable, run):
    indices = list(run.keys())
    bandwidth = [run[i]["loopix_total_bandwidth_mb"] for i in indices]

    plt.plot(indices, bandwidth, marker='o', linestyle='-', color='green', label='Bandwidth')

    plt.xlabel(variable)
    plt.ylabel("Bytes")
    plt.ylim(0, max(bandwidth) * 1.1)
    plt.title(f"Total Network Usage over {duration} seconds")
    plt.legend()
    plt.tight_layout()

    save_plot_directory(f"{directory}/plots/{variable}")
    plt.savefig(f"{directory}/plots/{variable}/bandwidth.png")
    plt.clf()

if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("Usage: python plot_data.py <directory> <path_length> <duration>")
        sys.exit(1)

    directory = sys.argv[1]
    path_length = int(sys.argv[2])
    duration = int(sys.argv[3])
    data = get_data(directory)

    for variable, run in data.items():
        plot_latency_components(directory, path_length, variable, run)
        plot_reliability(directory, variable, run)
        plot_incoming_messages(directory, variable, run)
        plot_bandwidth(directory, duration, variable, run)
