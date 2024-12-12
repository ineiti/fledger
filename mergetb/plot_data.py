import matplotlib.pyplot as plt
import numpy as np
import json

metrics_to_extract = [
    "loopix_bandwidth_bytes",
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
    for metric in metrics_to_extract:
        print(metric)
        print(data[metric])
        if metric == "loopix_bandwidth_bytes":
            data[metric] = np.mean(data[metric])
        else:
            print(f"sum: {data[metric]['sum']}")
            print(f"count: {data[metric]['count']}")
            data[metric] = np.sum(data[metric]["sum"]) / np.sum(data[metric]["count"])
        print(data[metric])
    return data


data = get_data()
average_data = calculate_average_data(data)
print(average_data)

# Data
categories = ['A', 'B', 'C', 'D']
groups = [
    [4, 7, 6, 5],  # Group 1
    [3, 8, 5, 6],  # Group 2
    [2, 6, 4, 4],  # Group 3
]
totals = [15, 28, 20, 22]  # Example totals that exceed the stacked sums

# Bar positions
x = np.arange(len(categories))
bar_width = 0.4  # Width for all bars

# Initialize bottom array to keep track of stacking
bottom = np.zeros(len(categories))

# Plot each group dynamically
for i, group in enumerate(groups):
    plt.bar(x, group, bottom=bottom, width=bar_width, label=f'Group {i + 1}')
    bottom += np.array(group)  # Update the bottom for the next stack

# Plot totals with a distinct color
plt.bar(
    x, 
    totals, 
    width=bar_width, 
    color='skyblue', 
    alpha=0.6, 
    edgecolor='black', 
    linewidth=1.5, 
    label='Total'
)

# Annotate totals
for i, total in enumerate(totals):
    plt.text(i, total + 0.5, str(total), ha='center', va='bottom', fontsize=10, color='black')

# Add labels and title
plt.xlabel('Categories')
plt.ylabel('Values')
plt.title('Stacked Bar Graph with Colored Totals')
plt.xticks(x, categories)
plt.legend()

# Save the plot to a file
plt.savefig('plot.png', dpi=300, bbox_inches='tight')

