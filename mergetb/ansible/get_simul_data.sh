#!/bin/bash

ansible-playbook -i inventory.ini delete_docker.yml
wait

ansible-playbook -i inventory.ini delete_only_metrics.yml
wait

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
lambda_payloads=(2 2.25 2.5 2.75 3 3.25 3.5 3.75 4 4.25 4.5 4.75 5 5.25 5.5 5.75 6 6.25 6.5 6.75 7 7.25 7.5 7.75 8 8.25 8.5 8.75 9 9.25 9.5 9.75 10)
chaff_lambdas=(2.2 2.175 2.15 2.125 2.1 2.075 2.05 2.025 2 1.975 1.95 1.925 1.9 1.875 1.85 1.825 1.8 1.775 1.75 1.725 1.7 1.675 1.65 1.625 1.6 1.575 1.55 1.525 1.5 1.475 1.45 1.425 1.4 1.375 1.35 1.325)
# lambda_payloads=(1 1.2)
# chaff_lambdas=(0.5 0.6)
mkdir -p metrics/lambda_payload

lambdas="{"
for i in "${!lambda_payloads[@]}"; do
    lambda_payload=${lambda_payloads[$i]}
    lambda_drop=${chaff_lambdas[$i]}
    lambdas+="\"$i\": {\"lambda_payload\": $lambda_payload, \"chaff_lambda\": $lambda_drop},"
done
lambdas="${lambdas%,}}"
echo -e "$lambdas" > metrics/lambda_payload/lambda_payload.json  >&2

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

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=$initial_path_length n_clients=3 duplicates=1 variable=lambda_payload index=$i"
    wait

    ansible-playbook -i inventory.ini stop_containers.yml 
    wait

    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait
done

# Try different mean_delay values
mean_delays=(50 60 70 80 90 100 110 120 130 140 150 160 170 180 190 200)
chaff_values=(4 3.3 2.79 2.45 2.2 2 1.79 1.62 1.48 1.37 1.27 1.18 1.11 1.05 0.99 0.95)
payload_values=(8 7.2 6.4 5.6 4.8 4 3.83333333 3.66666667 3.5 3.33333333 3.16666667 3 2.83333333 2.66666667 2.5)

# mean_delays=(50 60)
# chaff_values=(4 3.3)
# payload_values=(8 7.2)

# mu value = 20 16.67 14.29 12.5 11.11 10 9.09 8.33 7.69 7.14 6.67 6.25 5.88 5.56 5.26 5
# required number of messages per second = 48 24

mkdir -p metrics/mean_delay
mean_delay_json="{"

for i in "${!mean_delays[@]}"; do
    mean_delay=${mean_delays[$i]}
    lambda_drop=${chaff_values[$i]}
    lambda_payload=${payload_values[$i]}
    mean_delay_json+="\"$i\": {\"mean_delay\": $mean_delay, \"chaff_value\": $lambda_drop, \"payload_value\": $lambda_payload},"
done

mean_delay_json="${mean_delay_json%,}}"
echo -e "$mean_delay_json" > metrics/mean_delay/mean_delay.json  >&2

for i in "${!mean_delays[@]}"; do
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

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=$initial_path_length n_clients=3 duplicates=1 variable=mean_delay index=$i"
    wait

    ansible-playbook -i inventory.ini stop_containers.yml 
    wait

    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait
done

# # Try multiple measurements with the same values
time_pulls=(0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7)
mkdir -p metrics/control_2

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

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=$initial_path_length n_clients=3 duplicates=1 variable=control_2 index=$i"
    wait

    ansible-playbook -i inventory.ini stop_containers.yml 
    wait

    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait
done
