#!/bin/bash

if [ $# -lt 1 ]; then
    echo "Usage: $0 <token>"
    exit 1
fi

token=$1

set -u

# Define initial values
lambda_loop=1    
lambda_drop=1
lambda_payload=5
path_length=3
n_clients=3
mean_delay=100
lambda_loop_mix=1
time_pull=1
max_retrieve=5
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

# # Try different lambda_payload values
# lambda/mu values: 10 11 12 13 14 15 16 17 18 19 20 21 22
# messages sent per second: 30 33 36 39 42 45 48 51 54 57 60 63 66
chaff_lambdas=(0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1 1.1 1.2 1.3 1.4 1.5)

mkdir -p metrics/lambda_loop

lambdas="{"
for i in "${!chaff_lambdas[@]}"; do
    lambda_loop=${chaff_lambdas[$i]}
    lambdas+="\"$i\": {\"lambda_loop\": $lambda_loop},"
done
lambdas="${lambdas%,}}"
echo -e "$lambdas" > metrics/lambda_loop/lambda_loop.json  >&2

for i in "${!chaff_lambdas[@]}"; do
    lambda_loop=${chaff_lambdas[$i]}
    lambda_drop=${chaff_lambdas[$i]}
    lambda_loop_mix=${chaff_lambdas[$i]}

    cat <<EOL > loopix_core_config.yaml
---
lambda_loop: $lambda_loop
lambda_drop: $lambda_drop
lambda_payload: $initial_lambda_payload
path_length: $initial_path_length
mean_delay: $initial_mean_delay
lambda_loop_mix: $lambda_loop_mix
time_pull: $initial_time_pull
max_retrieve: $initial_max_retrieve
pad_length: $initial_pad_length
EOL

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=$initial_path_length token=$token n_clients=$n_clients duplicates=1 variable=lambda_loop index=$i"
    wait

    ansible-playbook -i inventory.ini stop_containers.yml 
    wait

    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait
done

# Try multiple measurements with the same values
time_pulls=(0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7)
mkdir -p metrics/control_mu

for i in "${!time_pulls[@]}"; do
    time_pull=${time_pulls[$i]}
    cat <<EOL > loopix_core_config.yaml
---
lambda_loop: $initial_lambda_loop
lambda_drop: $initial_lambda_drop
lambda_payload: $initial_lambda_payload
path_length: $initial_path_length
mean_delay: $initial_mean_delay
lambda_loop_mix: $initial_lambda_loop_mix
time_pull: $time_pull
max_retrieve: $initial_max_retrieve
pad_length: $initial_pad_length
EOL

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=$initial_path_length token=$token n_clients=$n_clients duplicates=1 variable=control_mu index=$i"
    wait

    ansible-playbook -i inventory.ini stop_containers.yml 
    wait

    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait
done
