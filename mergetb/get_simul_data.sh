#!/bin/bash

# Define initial values
lambda_loop=30.0
lambda_drop=30.0
lambda_payload=90.0
path_length=2
mean_delay=2000
lambda_loop_mix=30.0
time_pull=0.1
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

# Try different lambda_payload values
lambda_payloads=(30 40 50 60 70 80 90 100 110 120 130 140 150)
mkdir -p metrics/lambda_payload

# Prepare JSON object
lambda_payload_json="{"

for i in "${!lambda_payloads[@]}"; do
    lambda_payload=${lambda_payloads[$i]}
    lambda_payload_json+="\"$i\": $lambda_payload,"

    cat <<EOL > loopix_core_config.yaml
---
lambda_loop: $lambda_loop
lambda_drop: $lambda_drop
lambda_payload: $lambda_payload
path_length: $path_length
mean_delay: $mean_delay
lambda_loop_mix: $lambda_loop_mix
time_pull: $time_pull
max_retrieve: $max_retrieve
pad_length: $pad_length
EOL

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=2 variable=lambda_payload index=$i"
done

# Finalize JSON (remove trailing comma and close)
lambda_payload_json="${lambda_payload_json%,}}"
echo -e "$lambda_payload_json" > metrics/lambda_payload/lambda_payload.json


# Try different max_retrieve values
lambda_loop=$initial_lambda_loop
max_retrieves=(1 3 5 7 9)
mkdir -p metrics/max_retrieve
max_retrieve_json="{"

for i in "${!max_retrieves[@]}"; do
    max_retrieve=${max_retrieves[$i]}
    max_retrieve_json+="\"$i\": $max_retrieve,"

    cat <<EOL > loopix_core_config.yaml
---
lambda_loop: $lambda_loop
lambda_drop: $lambda_drop
lambda_payload: $lambda_payload
path_length: $path_length
mean_delay: $mean_delay
lambda_loop_mix: $lambda_loop_mix
time_pull: $time_pull
max_retrieve: $max_retrieve
pad_length: $pad_length
EOL

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=2 variable=max_retrieve index=$i"
done

max_retrieve_json="${max_retrieve_json%,}}"
echo -e "$max_retrieve_json" > metrics/max_retrieve/max_retrieve.json


# Try different mean_delay values
lambda_loop=$initial_lambda_loop
mean_delays=(500 1000 2000 4000 20000 100000 300000)
mkdir -p metrics/mean_delay
mean_delay_json="{"

for i in "${!mean_delays[@]}"; do
    mean_delay=${mean_delays[$i]}
    mean_delay_json+="\"$i\": $mean_delay,"

    cat <<EOL > loopix_core_config.yaml
---
lambda_loop: $lambda_loop
lambda_drop: $lambda_drop
lambda_payload: $lambda_payload
path_length: $path_length
mean_delay: $mean_delay
lambda_loop_mix: $lambda_loop_mix
time_pull: $time_pull
max_retrieve: $max_retrieve
pad_length: $pad_length
EOL

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=2 variable=mean_delay index=$i"
done

mean_delay_json="${mean_delay_json%,}}"
echo -e "$mean_delay_json" > metrics/mean_delay/mean_delay.json


# Try different time_pull values
lambda_loop=$initial_lambda_loop
time_pulls=(0.01 0.05 0.1 0.5 1 2)
mkdir -p metrics/time_pull
time_pull_json="{"

for i in "${!time_pulls[@]}"; do
    time_pull=${time_pulls[$i]}
    time_pull_json+="\"$i\": $time_pull,"

    cat <<EOL > loopix_core_config.yaml
---
lambda_loop: $lambda_loop
lambda_drop: $lambda_drop
lambda_payload: $lambda_payload
path_length: $path_length
mean_delay: $mean_delay
lambda_loop_mix: $lambda_loop_mix
time_pull: $time_pull
max_retrieve: $max_retrieve
pad_length: $pad_length
EOL

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=2 variable=time_pull index=$i"
done

time_pull_json="${time_pull_json%,}}"
echo -e "$time_pull_json" > metrics/time_pull/time_pull.json
