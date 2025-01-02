import matplotlib.pyplot as plt
import numpy as np
import json
import sys
import os

x_axis_name = {"lambda_loop": "Loop and Drop Messages per Second", "lambda_payload": "Payload Messages per Second from the Client"}


def get_data(directory):
    with open(os.path.join(directory, 'average_data.json'), 'r') as f:
        return json.load(f)

def convert_to_milliseconds(seconds):
    return seconds * 1000

def save_plot_directory(directory):
    os.makedirs(directory, exist_ok=True)

def plot_latency_components(directory, path_length, variable, run):
    plt.rcParams.update({'font.size': 16})
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
    if variable in x_axis_name:
        plt.xlabel(x_axis_name[variable])
    else:
        plt.xlabel(variable)
    plt.ylabel("Latency (milliseconds)")
    plt.title(f"End-to-End Latency with different components")
    plt.legend()
    plt.tight_layout()

    save_plot_directory(f"{directory}/plots/{variable}")
    plt.savefig(f"{directory}/plots/{variable}/latency_components.png")
    plt.clf()
    plt.close()

def plot_reliability(directory, variable, run):
    plt.rcParams.update({'font.size': 16})
    indices = list(run.keys())
    reliability = [run[i]["loopix_reliability"]*100 for i in indices]
    plt.figure(figsize=(20, 6))

    plt.plot(indices, reliability, marker='o', linestyle='-', color='blue', label='Reliability')
    if variable in x_axis_name:
        plt.xlabel(x_axis_name[variable])
    else:
        plt.xlabel(variable)
    plt.ylabel("Reliability (%)")
    plt.title(f"Percentage of Successful Web Proxy Requests")
    plt.ylim(0, 100)
    plt.legend()

    plt.tight_layout()

    save_plot_directory(f"{directory}/plots/{variable}")
    plt.savefig(f"{directory}/plots/{variable}/reliability.png")
    plt.clf()
    plt.close()

def plot_latency(directory, variable, run):
    plt.rcParams.update({'font.size': 16})
    keys = list(run.keys())
    latency = [run[i]["loopix_end_to_end_latency_seconds"] for i in keys]
    plt.figure(figsize=(20, 6))

    plt.plot(keys, latency, marker='o', linestyle='-', color='green', label='Latency')

    if variable in x_axis_name:
        plt.xlabel(x_axis_name[variable])
    else:
        plt.xlabel(variable)
    plt.ylabel("Latency (seconds)")
    plt.ylim(0, max(latency) * 1.1)
    plt.title(f"End-to-End Latency")
    plt.legend()

    plt.tight_layout()

    save_plot_directory(f"{directory}/plots/{variable}")
    plt.savefig(f"{directory}/plots/{variable}/latency.png")
    plt.clf()
    plt.close()

def plot_latency_and_bandwidth(directory, variable, run):
    plt.rcParams.update({'font.size': 16})
    keys = list(run.keys()) 
    x_values = np.array([float(k) for k in keys])  
    
    latency = [run[k]["loopix_end_to_end_latency_seconds"] for k in keys]
    
    bandwidth = [run[k]["loopix_total_bandwidth_mb"] for k in keys]

    plt.figure(figsize=(20, 6))

    fig, ax1 = plt.subplots(figsize=(20, 6))

    ax1.plot(x_values, latency, marker='o', linestyle='-', color='green', label='Latency (s)')
    if variable in x_axis_name:
        ax1.set_xlabel(x_axis_name[variable])
    else:
        ax1.set_xlabel(variable)
    ax1.set_ylabel("Latency (seconds)", color='green')
    ax1.tick_params(axis='y', labelcolor='green')

    ax1.set_xticks(x_values)

    ax2 = ax1.twinx()
    ax2.plot(x_values, bandwidth, marker='o', linestyle='--', color='blue', label='Bandwidth (MB)')
    ax2.set_ylabel("Bandwidth (MB)", color='blue')
    ax2.tick_params(axis='y', labelcolor='blue')


    fig.suptitle(f"Latency and Bandwidth")
    ax1.legend(loc='upper left')
    ax2.legend(loc='upper right')

    ax1.set_ylim(0, max(latency) * 1.1)
    ax2.set_ylim(0, max(bandwidth) * 1.1)

    plt.tight_layout()
    save_plot_directory(f"{directory}/plots/{variable}")
    plt.savefig(f"{directory}/plots/{variable}/latency_and_bandwidth.png")
    plt.clf()
    plt.close()

def plot_reliability_latency(directory, variable, run):
    plt.rcParams.update({'font.size': 16})
    keys = list(run.keys()) 
    x_values = np.array([float(k) for k in keys])  
    
    latency = [run[k]["loopix_end_to_end_latency_seconds"] for k in keys]
    
    reliability = [run[k]["loopix_reliability"] * 100 for k in keys]

    plt.figure(figsize=(20, 6))

    fig, ax1 = plt.subplots(figsize=(20, 6))

    ax1.plot(x_values, reliability, marker='o', linestyle='-', color='green', label='Reliability (%)')
    if variable in x_axis_name:
        ax1.set_xlabel(x_axis_name[variable])
    else:
        ax1.set_xlabel(variable)
    ax1.set_ylabel("Percentage of Successful Proxy Requests (%)", color='green')
    ax1.tick_params(axis='y', labelcolor='green')

    ax1.set_xticks(x_values)

    ax2 = ax1.twinx()
    ax2.plot(x_values, latency, marker='o', linestyle='--', color='blue', label='Latency (s)')
    ax2.set_ylabel("Latency (seconds)", color='blue')
    ax2.tick_params(axis='y', labelcolor='blue')


    fig.suptitle(f"Latency and Reliability")
    ax1.legend(loc='upper left')
    ax2.legend(loc='upper right')

    ax1.set_ylim(0, max(reliability) * 1.1)
    ax2.set_ylim(0, max(latency) * 1.1)

    plt.tight_layout()
    save_plot_directory(f"{directory}/plots/{variable}")
    plt.savefig(f"{directory}/plots/{variable}/reliability_latency.png")
    plt.clf()
    plt.close()

def plot_reliability_incoming_latency(directory, variable, run):
    plt.rcParams.update({'font.size': 16})
    keys = list(run.keys())  
    x_values = np.array([float(k) for k in keys])  
    
    latency = [run[k]["loopix_end_to_end_latency_seconds"] for k in keys]
    reliability = [run[k]["loopix_reliability"] * 100 for k in keys] 
    incoming_messages = [run[k]["loopix_incoming_messages"] for k in keys]

    fig, ax1 = plt.subplots(figsize=(15, 6))

    ax1.plot(x_values, incoming_messages, marker='o', linestyle=':', color='blue', label='Incoming Messages')
    ax1.set_ylabel("Number of Incoming Messages per Second per Mixnode", color='blue')
    ax1.tick_params(axis='y', labelcolor='blue')

    ax2 = ax1.twinx()
    ax2.plot(x_values, reliability, marker='o', linestyle='--', color='green', label='Reliability (%)')
    ax2.set_ylabel("Percentage of Successful Proxy Requests (%)", color='green')
    ax2.tick_params(axis='y', labelcolor='green')

    ax3 = ax1.twinx()
    ax3.plot(x_values, latency, marker='o', linestyle='-', color='red', label='Latency (s)')
    ax3.set_ylabel("End-to-End Latency (seconds)", color='red')
    ax3.tick_params(axis='y', labelcolor='red')

    fig.suptitle(f"Reliability, Incoming Messages, and Latency")
    ax1.legend(loc='upper left')
    ax2.legend(loc='upper center')
    ax3.legend(loc='upper right')

    ax1.set_xticks(x_values)
    ax1.set_ylim(0, max(incoming_messages) * 1.1) 
    ax2.set_ylim(0, max(reliability) * 1.1) 
    ax3.set_ylim(0, max(latency) * 1.1) 

    plt.subplots_adjust(left=0.1)

    save_plot_directory(f"{directory}/plots/{variable}")
    plt.savefig(f"{directory}/plots/{variable}/reliability_incoming_latency.png")
    plt.clf()
    plt.close()


def plot_incoming_messages(directory, variable, run):
    plt.rcParams.update({'font.size': 16})
    indices = list(run.keys())
    incoming_messages = [run[i]["loopix_incoming_messages"] for i in indices]
    incoming_messages_std = [run[i]["loopix_incoming_messages_std"] for i in indices]
    plt.figure(figsize=(20, 6))
    
    plt.plot(indices, incoming_messages, marker='o', linestyle='-', color='blue', label='Incoming Messages')
    plt.errorbar(indices, incoming_messages, yerr=incoming_messages_std, fmt='o', color='blue')
    if variable in x_axis_name:
        plt.xlabel(x_axis_name[variable])
    else:
        plt.xlabel(variable)
    plt.ylabel("Incoming Messages")
    plt.ylim(0, max(incoming_messages) * 1.1)
    plt.title(f"Number of Incoming Messages per Second per Mixnode")
    plt.legend()

    plt.tight_layout()

    save_plot_directory(f"{directory}/plots/{variable}")
    plt.savefig(f"{directory}/plots/{variable}/incoming_messages.png")
    plt.clf()
    plt.close()

def plot_bandwidth(directory, duration, variable, run):
    plt.rcParams.update({'font.size': 16})
    keys = list(run.keys())
    indices = [float(i) for i in keys]
    bandwidth = [run[i]["loopix_total_bandwidth_mb"] for i in keys]

    coefficients = np.polyfit(indices, bandwidth, 1)
    fitted_line = np.poly1d(coefficients)
    plt.figure(figsize=(20, 6))

    plt.plot(indices, bandwidth, marker='o', linestyle='-', color='green', label='Bandwidth')
    plt.plot(indices, fitted_line(indices), linestyle='--', color='blue', label='Fit Line')

    equation_text = f"y = {coefficients[0]:.2f}x + {coefficients[1]:.2f}"
    plt.text(0.05, 0.95, equation_text, transform=plt.gca().transAxes, fontsize=10,
             verticalalignment='top', bbox=dict(boxstyle="round", facecolor="white", alpha=0.5))

    if variable in x_axis_name:
        plt.xlabel(x_axis_name[variable])
    else:
        plt.xlabel(variable)
    plt.ylabel("Bytes")
    plt.ylim(0, max(bandwidth) * 1.1)
    plt.title(f"Total Network Usage over {duration} seconds")
    plt.legend()
    plt.tight_layout()

    save_plot_directory(f"{directory}/plots/{variable}")
    plt.savefig(f"{directory}/plots/{variable}/bandwidth.png")
    plt.clf()
    plt.close()



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
        plot_latency(directory, variable, run)
        plot_bandwidth(directory, duration, variable, run)
        plot_latency_and_bandwidth(directory, variable, run)
        # plot_reliability_incoming_latency(directory, variable, run)
        plot_reliability_latency(directory, variable, run)