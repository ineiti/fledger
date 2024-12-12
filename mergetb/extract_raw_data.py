import os
import re
import json
import sys

metrics_to_extract = [
    "loopix_bandwidth_bytes",
    "loopix_end_to_end_latency_seconds",
    "loopix_encryption_latency_milliseconds",
    "loopix_client_delay_milliseconds",
    "loopix_decryption_latency_milliseconds",
    "loopix_mixnode_delay_milliseconds",
    "loopix_provider_delay_milliseconds",
]

results = {}
for metric in metrics_to_extract:
    if metric == "loopix_bandwidth_bytes":
        results[metric] = []
    else:
        results[metric] = {"sum": [], "count": []}

if len(sys.argv) != 2:
    print("Usage: python extract_raw_data.py <path_length>")
    sys.exit(1)

path_length = int(sys.argv[1])

for i in range(path_length*path_length + path_length * 2):
    node_dir = f"./mergetb_data/node-{i}/data"
    metrics_file = os.path.join(node_dir, "metrics.txt")
    
    if os.path.exists(metrics_file):
        with open(metrics_file, 'r') as f:
            content = f.read()
        
        for metric in metrics_to_extract:
            if metric == "loopix_bandwidth_bytes":
                pattern = rf"{metric}\s+([0-9.e+-]+)$"
                match = re.search(pattern, content, re.MULTILINE)
                if match:
                    results[metric].append(float(match.group(1)))
                else:
                    print(f"Error for node-{i}: match {match}")
            else:
                pattern_sum = rf"^{metric}_sum\s+([0-9.e+-]+)"
                pattern_count = rf"^{metric}_count\s+([0-9.e+-]+)"
                match_sum = re.search(pattern_sum, content, re.MULTILINE)
                match_count = re.search(pattern_count, content, re.MULTILINE)
                if match_count and match_sum:
                    results[metric][f"sum"].append(float(match_sum.group(1)))
                    results[metric][f"count"].append(float(match_count.group(1)))
                elif match_count or match_sum:
                    print(f"Error for node-{i}: match_sum {match_sum}, match_count {match_count}")
        

with open('raw_metrics.json', 'w') as f:
    json.dump(results, f, indent=2)
