#!/bin/bash

if [ $# -lt 1 ]; then
    echo "Usage: $0 <token>"
    exit 1
fi

token=$1

# Define initial values
lambda_loop=1.15 
lambda_drop=1.15
lambda_payload=6.1
path_length=3
mean_delay=100
lambda_loop_mix=1.15
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
lambda_payloads=(2.0 2.25 2.5 2.75 3.0 3.25 3.5 3.75 4.0 4.25 4.5 4.75 5.0 5.25 5.5 5.75 6.0 6.25 6.5 6.75 7.0 7.25 7.5 7.75 8.0 8.25 8.5 8.75 9.0 9.25 9.5 9.75 10.0)
chaff_lambdas=(1.95 1.9 1.85 1.8 1.75 1.7 1.65 1.6 1.55 1.5 1.45 1.4 1.35 1.3 1.25 1.2 1.15 1.118 1.086 1.054 1.022 0.99 0.958 0.926 0.894 0.862 0.83 0.798 0.766 0.734 0.702 0.67 0.638 0.606)
# lambda_payloads=(1 1.2)
# chaff_lambdas=(0.5 0.6)
mkdir -p metrics/lambda_payload

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

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=$initial_path_length n_clients=3 token=$token duplicates=1 variable=lambda_payload index=$i"
    wait

    ansible-playbook -i inventory.ini stop_containers.yml 
    wait

    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait
done


# Try different mean_delay values
mean_delays=(50 60 70 80 90 100 110 120 130 140 150 160 170 180 190 200)
payload_values=(6.1 6.1 6.1 6.1 6.1 6.1 6.1 6.1 6.1 6.1 6.1 6.1 6.1 6.1 6.1 6.1)
chaff_values=(3.65 3.15 2.65 2.15 1.65 1.15 1.06 0.97 0.88 0.8 0.71 0.62 0.53 0.44 0.35 0.26)

mkdir -p metrics/mean_delay

start_index=0
end_index=16

for ((i=start_index; i < end_index; i++)); do
    mean_delay=${mean_delays[$i]}
    lambda_drop=${chaff_values[$i]}
    lambda_loop=${chaff_values[$i]}
    lambda_loop_mix=${chaff_values[$i]}
    lambda_payload=${payload_values[$i]}

    cat <<EOL > loopix_core_config.yaml
---
lambda_loop: $lambda_loop
lambda_drop: $lambda_drop
lambda_payload: $lambda_payload
path_length: $initial_path_length
mean_delay: $mean_delay
lambda_loop_mix: $lambda_loop_mix
time_pull: $initial_time_pull
max_retrieve: $initial_max_retrieve
pad_length: $initial_pad_length
EOL

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=$initial_path_length n_clients=3 duplicates=1 token=$token variable=mean_delay index=$i"
    wait

    ansible-playbook -i inventory.ini stop_containers.yml 
    wait

    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait
done

# # Try multiple measurements with the same values
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

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=$initial_path_length n_clients=3 duplicates=1 token=$token variable=control index=$i"
    wait

    ansible-playbook -i inventory.ini stop_containers.yml 
    wait

    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait
done
