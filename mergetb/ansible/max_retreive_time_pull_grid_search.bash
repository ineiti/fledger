#!/bin/bash

if [ $# -lt 1 ]; then
    echo "Usage: $0 <token>"
    exit 1
fi

token=$1

# Define initial values
lambda_loop=1.65
lambda_drop=1.65
lambda_payload=6.1
path_length=3
mean_delay=80
lambda_loop_mix=1.65
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
time_pulls=(0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0 1.2 1.4 1.6 1.8 2.0)
max_retrieves=(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15)

# time_pulls=(0.2 0.5)
# max_retrieves=(1)
mkdir -p metrics/grid_search

grid_search_json="{"
for i in "${!time_pulls[@]}"; do
    for j in "${!max_retrieves[@]}"; do
        time_pull=${time_pulls[$i]}
        max_retrieve=${max_retrieves[$j]}
        grid_search_json+="\"$i-$j\": {\"time_pull\": $time_pull, \"max_retrieve\": $max_retrieve},"
    done
done
grid_search_json="${grid_search_json%,}}"
echo -e "$grid_search_json" > metrics/grid_search/grid_search.json

for i in "${!time_pulls[@]}"; do
    for j in "${!max_retrieves[@]}"; do
        time_pull=${time_pulls[$i]}
        max_retrieve=${max_retrieves[$j]}

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
        ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=$initial_path_length n_clients=3 token=$token duplicates=1 variable=grid_search index=\"$i-$j\""
        wait

        ansible-playbook -i inventory.ini stop_containers.yml 
        wait

        ansible-playbook -i inventory.ini delete_only_metrics.yml
        wait
    done
done


