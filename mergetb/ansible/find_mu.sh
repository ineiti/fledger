#!/bin/bash

# Define initial values
lambda_loop=2    
lambda_drop=2
lambda_payload=4
path_length=3
mean_delay=100
lambda_loop_mix=2
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
# lambda/mu values: 10 12 14 16 18 20 22 24 26 28 30
lambda_payloads=(3.33 4 4.67 5.33 6 6.67 7.33 8 8.67 9.33 10)
chaff_lambdas=(0 0 0 0 0 0 0 0 0 0 0)
mkdir -p metrics/mu

lambdas="{"
for i in "${!lambda_payloads[@]}"; do
    lambda_payload=${lambda_payloads[$i]}
    lambda_drop=${chaff_lambdas[$i]}
    lambdas+="\"$i\": {\"lambda_payload\": $lambda_payload, \"chaff_lambda\": $lambda_drop},"
done
lambdas="${lambdas%,}}"
echo -e "$lambdas" > metrics/mu/mu.json  >&2

for i in "${!lambda_payloads[@]}"; do
    lambda_payload=${lambda_payloads[$i]}
    lambda_drop=${chaff_lambdas[$i]}
    lambda_loop=${chaff_lambdas[$i]}
    lambda_loop_mix=${chaff_lambdas[$i]}

    cat <<EOL > loopix_core_config.yaml
---
lambda_loop: $lambda_loop
lambda_drop: $lambda_drop
lambda_payload: $lambda_payload
path_length: $initial_path_length
mean_delay: $initial_mean_delay
lambda_loop_mix: $lambda_loop_mix
time_pull: $initial_time_pull
max_retrieve: $initial_max_retrieve
pad_length: $initial_pad_length
EOL

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=$initial_path_length n_clients=3 duplicates=1 variable=mu index=$i"
    wait

    ansible-playbook -i inventory.ini stop_containers.yml 
    wait

    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait
done

# Try multiple measurements with the same values
time_pulls=(0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7)
mkdir -p metrics/control

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

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=$initial_path_length n_clients=3 duplicates=1 variable=control index=$i"
    wait

    ansible-playbook -i inventory.ini stop_containers.yml 
    wait

    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait
done
