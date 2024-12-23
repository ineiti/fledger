#!/bin/bash

# Define initial values
lambda_loop=30.0
lambda_drop=30.0
lambda_payload=90.0
path_length=2
mean_delay=2000
lambda_loop_mix=30.0
time_pull=0.7
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
lambda_payloads=(40 45 50 55 60 65 70 75 80 85 90 95 100 105 110 115 120 125 130 135 140 145 150 160 170 180 190 200)
mkdir -p metrics/lambda_payload

# Prepare JSON object
lambda_payload_json="{"

for i in "${!lambda_payloads[@]}"; do
    lambda_payload=${lambda_payloads[$i]}
    lambda_payload_json+="\"$i\": $lambda_payload,"

    cat <<EOL > loopix_core_config.yaml
---
lambda_loop: $initial_lambda_loop
lambda_drop: $initial_lambda_drop
lambda_payload: $lambda_payload
path_length: $initial_path_length
mean_delay: $initial_mean_delay
lambda_loop_mix: $initial_lambda_loop_mix
time_pull: $initial_time_pull
max_retrieve: $initial_max_retrieve
pad_length: $initial_pad_length
EOL

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=2 variable=lambda_payload index=$i"
done

# Finalize JSON (remove trailing comma and close)
lambda_payload_json="${lambda_payload_json%,}}"
echo -e "$lambda_payload_json" > metrics/lambda_payload/lambda_payload.json

ansible-playbook -i inventory.ini delete_docker.yml 

# Try different max_retrieve values
lambda_loop=$initial_lambda_loop
max_retrieves=(1 2 3 4 5 6 7 8 9 10 11 12)
mkdir -p metrics/max_retrieve
max_retrieve_json="{"

for i in "${!max_retrieves[@]}"; do
    max_retrieve=${max_retrieves[$i]}
    max_retrieve_json+="\"$i\": $max_retrieve,"

    cat <<EOL > loopix_core_config.yaml
---
lambda_loop: $initial_lambda_loop
lambda_drop: $initial_lambda_drop
lambda_payload: $initial_lambda_payload
path_length: $initial_path_length
mean_delay: $initial_mean_delay
lambda_loop_mix: $initial_lambda_loop_mix
time_pull: $initial_time_pull
max_retrieve: $max_retrieve
pad_length: $initial_pad_length
EOL

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=2 variable=max_retrieve index=$i"
done

max_retrieve_json="${max_retrieve_json%,}}"
echo -e "$max_retrieve_json" > metrics/max_retrieve/max_retrieve.json

ansible-playbook -i inventory.ini delete_docker.yml 

# Try different mean_delay values
lambda_loop=$initial_lambda_loop
mean_delays=(1 2 5 10 20 40 80 100 120 140 160 180 200 220 240 260 280 300 320 340 360 380 400 420 440 460 480 500)
mkdir -p metrics/mean_delay
mean_delay_json="{"

for i in "${!mean_delays[@]}"; do
    mean_delay=${mean_delays[$i]}
    mean_delay_json+="\"$i\": $mean_delay,"

    cat <<EOL > loopix_core_config.yaml
---
lambda_loop: $initial_lambda_loop
lambda_drop: $initial_lambda_drop
lambda_payload: $initial_lambda_payload
path_length: $initial_path_length
mean_delay: $mean_delay
lambda_loop_mix: $initial_lambda_loop_mix
time_pull: $initial_time_pull
max_retrieve: $initial_max_retrieve
pad_length: $initial_pad_length
EOL

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=2 variable=mean_delay index=$i"
done

mean_delay_json="${mean_delay_json%,}}"
echo -e "$mean_delay_json" > metrics/mean_delay/mean_delay.json

ansible-playbook -i inventory.ini delete_docker.yml 

# Try different time_pull values
lambda_loop=$initial_lambda_loop
time_pulls=(0.05 0.1 0.15 0.2 0.25 0.3 0.35 0.4 0.45 0.5 0.55 0.6 0.65 0.7 0.75 0.8 0.85 0.9 0.95 1 1.25 1.5 1.75 2 2.25 2.5 3)
mkdir -p metrics/time_pull
time_pull_json="{"

for i in "${!time_pulls[@]}"; do
    time_pull=${time_pulls[$i]}
    time_pull_json+="\"$i\": $time_pull,"

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

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=2 variable=time_pull index=$i"
done

time_pull_json="${time_pull_json%,}}"
echo -e "$time_pull_json" > metrics/time_pull/time_pull.json

ansible-playbook -i inventory.ini delete_docker.yml

# Try multiple measurements with the same values
lambda_loop=$initial_lambda_loop
time_pulls=(0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7)
mkdir -p metrics/control
control_json="{"

for i in "${!time_pulls[@]}"; do
    time_pull=${time_pulls[$i]}
    control_json+="\"$i\": $time_pull,"

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

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=2 variable=control index=$i"
done

control_json="${control_json%,}}"
echo -e "$control_json" > metrics/control_json/control_json.json
