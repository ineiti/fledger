import json
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.ticker import LogLocator, ScalarFormatter, MaxNLocator

def calculate_average_latency(data_section, metric_type):
    averages = {}
    print(f"Calculating averages for {metric_type}")
    for key, values in data_section[metric_type].items():
        total_times = values.get("total_times", [])
        if total_times:
            total_times_in_seconds = [time / 1000 for time in total_times]
            averages[key] = np.mean(total_times_in_seconds)
            print(f"Average for {key}: {averages[key]}")
        else:
            averages[key] = 0
            print(f"No total_times found for {metric_type}, setting average to 0")
    return averages

def plot_latency(averages, title, metric_type, unit):
    plt.figure(figsize=(15, 6))
    plt.plot(list(averages.keys()), list(averages.values()), marker='o', linestyle='-')
    plt.title(f"Average Time to Retrieve a Web Page vs {title}")
    plt.xlabel(f"{metric_type} {unit}")
    plt.ylabel('Time (seconds)')
    plt.yscale('log')  
    plt.gca().yaxis.set_major_locator(LogLocator(base=10.0, subs='auto', numticks=10))
    plt.gca().yaxis.set_major_formatter(ScalarFormatter())
    plt.xticks(rotation=45)
    plt.grid(True, linestyle='--', alpha=0.7)
    plt.tight_layout()
    plt.savefig(f"./simul_data/graphs/latency_{title}.png")
    plt.close()


def plot_messages(data, title, metric_type, unit):
    forwarded = []
    sent = []
    received = []

    for key, value in data.items():
        forwarded.append(value.get("total_forwarded"))
        sent.append(value.get("total_sent"))
        received.append(value.get("total_received"))

    plt.figure(figsize=(15, 6))
    plt.plot(list(data.keys()), forwarded, marker='o', linestyle='-', label='Forwarded')
    plt.plot(list(data.keys()), sent, marker='o', linestyle='-', label='Sent')
    plt.plot(list(data.keys()), received, marker='o', linestyle='-', label='Received')
    plt.title(f"Number of Messages within 5 minutes vs {title}")
    plt.xlabel(f"{metric_type} {unit}")
    plt.ylabel('Number of messages')
    plt.gca().yaxis.set_major_locator(MaxNLocator(nbins=10))
    plt.gca().yaxis.set_major_formatter(ScalarFormatter())
    plt.xticks(rotation=45)
    plt.grid(True, linestyle='--', alpha=0.7)
    plt.tight_layout()
    plt.legend()
    plt.savefig(f"./simul_data/graphs/messages_{title}.png")
    plt.close()


def main():
    with open('./simul_data/data_cleaned.json', 'r') as file:
        data = json.load(file)

    metrics = ["lambda", "max_retrieve", "mean_delay", "path_length", "time_pull"]
    units = ["(messages/minute)", "(messages)", "(ms)", "(hops)", "(s)"]
    latencies = {metric: calculate_average_latency(data['latency'], metric) for metric in metrics}

    for metric, unit in zip(metrics, units):
        plot_latency(latencies[metric], f"{metric}", metric, unit)
        plot_messages(data["messages"][metric], f"{metric}", metric, unit)

if __name__ == "__main__":
    main()
