import matplotlib.pyplot as plt
import numpy as np
import json
import sys
import os

from plot_data import get_data, convert_to_milliseconds, save_plot_directory

x_axis_name = "loopix_end_to_end_latency_seconds"

def plot_latency_components(directory, path_length, variable, run):
    plt.rcParams.update({'font.size': 16})

    indices = list(run.keys())
    n_mixnode_delay = (path_length + 1) * 2
    n_decryption_delay = (path_length + 3) * 2
    n_network_delay = (path_length - 1) * 2 + 8

    provider_delay = np.array([run[i]["loopix_provider_delay_milliseconds"] * 2 for i in indices])
    mixnode_delay = np.array([run[i]["loopix_mixnode_delay_milliseconds"] * n_mixnode_delay for i in indices])
    decryption_delay = np.array([run[i]["loopix_decryption_latency_milliseconds"] * n_decryption_delay for i in indices])
    client_delay = np.array([run[i]["loopix_client_delay_milliseconds"] * 2 for i in indices])
    encryption_delay = np.array([run[i]["loopix_encryption_latency_milliseconds"] * 2 for i in indices])
    network_delay = np.full_like(provider_delay, 15 * n_network_delay)
    total_latency = np.array([convert_to_milliseconds(run[i]["loopix_end_to_end_latency_seconds"]) for i in indices])

    bar_width = 0.6
    plt.figure(figsize=(8, 12))

    components = {
        "Encryption Delay": encryption_delay,
        "Client Delay": client_delay,
        "Decryption Delay": decryption_delay,
        "Mixnode Delay": mixnode_delay,
        "Provider Delay": provider_delay,
        "Network Delay": network_delay
    }

    means = {key: np.mean(value) for key, value in components.items()}

    bottom = 0
    legend_labels = ["Encryption Delay", "Client Delay", "Decryption Delay", "Mixnode Delay", "Provider Delay", "Network Delay"] 

    for key in legend_labels:
        bar = plt.bar(
            0,
            means[key],
            width=bar_width,
            label=key,
            bottom=bottom
        )
        bottom += means[key]

    components_sum = provider_delay + mixnode_delay + decryption_delay + client_delay + encryption_delay + network_delay
    components_sum_mean = np.mean(components_sum)
    components_sum_std = np.std(components_sum)



    total_latency_mean = np.mean(total_latency)
    total_latency_std = np.std(total_latency)

    plt.bar(0, total_latency_mean, width=bar_width, color='none', edgecolor='red', linestyle='--', linewidth=2, label="End-to-End Latency")

    plt.errorbar(
        0, total_latency_mean, yerr=total_latency_std,
        fmt='none', ecolor='red', capsize=5
    )

    plt.errorbar(
        0, components_sum_mean, yerr=components_sum_std,
        fmt='none', ecolor='blue', capsize=5
    )

    plt.xticks([0], ["Average Over 12 Simulations"])
    plt.ylabel("Latency (milliseconds)")
    plt.title("End-to-End Latency Components")
    plt.legend(loc='upper left', bbox_to_anchor=(1.05, 1), title="Latency Components") 
    plt.tight_layout(rect=[0, 0, 0.75, 1]) 


    save_plot_directory(directory)
    plt.savefig(os.path.join(directory, "average_latency_components.png"),   bbox_inches='tight')
    plt.clf()
    plt.close()






if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("Usage: python plot_data.py <directory> <path_length>")
        sys.exit(1)


    directory = sys.argv[1]
    path_length = int(sys.argv[2])
    data = get_data(directory)

    variable = "control"

    run = data[variable]

    plot_dir = os.path.join(directory, "plots", variable)

    plot_latency_components(plot_dir, path_length, variable, run)
