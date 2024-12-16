import matplotlib.pyplot as plt
import numpy as np
import json

metrics_to_extract = [
    "loopix_bandwidth_bytes",
    "loopix_number_of_proxy_requests",
    "loopix_end_to_end_latency_seconds",
    "loopix_encryption_latency_milliseconds",
    "loopix_client_delay_milliseconds",
    "loopix_decryption_latency_milliseconds",
    "loopix_mixnode_delay_milliseconds",
    "loopix_provider_delay_milliseconds",
]

def get_data():
    with open('raw_metrics.json', 'r') as f:
        data = json.load(f)
    return data

def calculate_average_data(data):
    results = {}
    for metric in metrics_to_extract:
        if metric in ["loopix_bandwidth_bytes", "loopix_number_of_proxy_requests"]:
            results[metric] = np.mean(data[metric])
        else:
            results[metric] = np.sum(data[metric]["sum"]) / np.sum(data[metric]["count"])
    return results

def create_and_save_stacked_bar_chart(average_data):
    categories = ['loopix_latency_milliseconds']
    components = [
        average_data["loopix_encryption_latency_milliseconds"],
        average_data["loopix_client_delay_milliseconds"],
        average_data["loopix_decryption_latency_milliseconds"],
        average_data["loopix_mixnode_delay_milliseconds"],
        average_data["loopix_provider_delay_milliseconds"]
    ]

    x = np.arange(len(categories))
    bar_width = 0.5

    bottom = np.zeros(len(categories))

    colors = ['blue', 'orange', 'green', 'red', 'purple']
    labels = [
        "Encryption Latency",
        "Client Delay",
        "Decryption Latency",
        "Mixnode Delay",
        "Provider Delay",
        "Network Latency"
    ]
    plt.figure(figsize=(3, 20))

    for i, component in enumerate(components):
        plt.bar(
            x,
            [component],
            bottom=bottom,
            color=colors[i],
            width=bar_width,
            label=labels[i]
        )
        bottom += [component]

    plt.xlabel('Categories')
    plt.ylabel('Values (Milliseconds)')
    plt.title('Stacked Bar Chart for Latency Components')
    plt.xticks(x, ['Total Latency'], rotation=0)
    plt.legend()

    plt.savefig('plot.png', dpi=300, bbox_inches='tight')


if __name__ == "__main__":
    data = get_data()

    average_data = {}

    for variable_name, runs in data.items():
        average_data[variable_name] = {}

        for index, metrics_data in runs.items():
            print(f"Calculating average data for {variable_name} {index}")
            average_data[variable_name][index] = calculate_average_data(metrics_data)

    with open('average_data.json', 'w') as f:
        json.dump(average_data, f, indent=2)

   

        
