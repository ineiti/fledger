#!/bin/bash

# Define initial values
lambda_loop=2
lambda_drop=2
lambda_payload=4
path_length=3
mean_delay=100
lambda_loop_mix=2
time_pull=1.0
max_retrieve=3
pad_length=150

# Save initial values
initial_lambda_loop=$lambda_loop
initial_lambda_drop=$lambda_drop
initial_lambda_payload=$lambda_payload
initial_path_length=$path_length
initial_mean_delay=$mean_delay
initial_lambda_loop_mix=$lambda_loop_mix
initial_time_pull=$time_pull
initial_max_retrieve=$max_retrieve
initial_pad_length=$pad_length

# Try different time_pull and max_retrieve values
time_pulls=(0.2 0.5 0.7 0.8 0.9 1.0 1.1 1.2 1.3 1.4 1.5 1.7 2.0)
max_retrieves=(1 2 3 4 5 6 7 8 9 10 11 12)
mkdir -p metrics/grid_search

# Prepare JSON object
grid_search_json="{"

for i in "${!time_pulls[@]}"; do
    for j in "${!max_retrieves[@]}"; do
        time_pull=${time_pulls[$i]}
        max_retrieve=${max_retrieves[$j]}
        grid_search_json+="\"$i-$j\": {\"time_pull\": $time_pull, \"max_retrieve\": $max_retrieve},"

        cat <<EOL > loopix_core_config.yaml
---
lambda_loop: $initial_lambda_loop
lambda_drop: $initial_lambda_drop
lambda_payload: $initial_lambda_payload
path_length: $initial_path_length
mean_delay: $initial_mean_delay
lambda_loop_mix: $initial_lambda_loop_mix
time_pull: $time_pull
max_retrieve: $max_retrieve
pad_length: $initial_pad_length
EOL
        ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=$initial_path_length n_clients=3 duplicates=1 variable=grid_search index=\"$i-$j\""
    done
done

# Finalize JSON (remove trailing comma and close)
grid_search_json="${grid_search_json%,}}"
echo -e "$grid_search_json" > metrics/grid_search/grid_search.json
