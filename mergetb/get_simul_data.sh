#!/bin/bash

# Define initial values
lambda_loop=90.0
lambda_drop=90.0
lambda_payload=90.0
path_length=2
mean_delay=2000
lambda_loop_mix=90.0
time_pull=0.1
max_retrieve=5
pad_length=150

cat <<EOL > loopix_core_config_initial.yaml
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

# Try different lambda values between 1 and 60
lambda_loops=(30 40 50 60 70 80 90 100 110 120 130 140 150)
lambda_drops=(30 40 50 60 70 80 90 100 110 120 130 140 150)
lambda_payloads=(30 40 50 60 70 80 90 100 110 120 130 140 150)
lambda_loop_mixes=(30 40 50 60 70 80 90 100 110 120 130 140 150)

mkdir -p metrics/lambda_loop

for i in "${!lambda_payloads[@]}"; do
    # lambda_loop=${lambda_loops[$i]}
    # lambda_drop=${lambda_drops[$i]}
    lambda_payload=${lambda_payloads[$i]}
    # lambda_loop_mix=${lambda_loop_mixes[$i]}

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

    ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=2 variable=lambda_loop index=$i"

done

# Try different max_retrieve values between 5 and 10
lambda_loop=$initial_lambda_loop
lambda_drop=$initial_lambda_drop
lambda_payload=$initial_lambda_payload
path_length=$initial_path_length
lambda_loop_mix=$initial_lambda_loop_mix
mean_delay=$initial_mean_delay
time_pull=$initial_time_pull
max_retrieve=$initial_max_retrieve
pad_length=$initial_pad_length

max_retrieves=(1 3 5 7 9)

mkdir -p metrics/max_retrieve

for i in "${!max_retrieves[@]}"; do
    max_retrieve=${max_retrieves[$i]}

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
    wait
done

# Try different mean_delay values using an array
lambda_loop=$initial_lambda_loop
lambda_drop=$initial_lambda_drop
lambda_payload=$initial_lambda_payload
path_length=$initial_path_length
lambda_loop_mix=$initial_lambda_loop_mix
mean_delay=$initial_mean_delay
time_pull=$initial_time_pull
max_retrieve=$initial_max_retrieve
pad_length=$initial_pad_length

mean_delays=(500 1000 2000 4000 20000 100000 300000)

mkdir -p metrics/mean_delay

for i in "${!mean_delays[@]}"; do
    mean_delay=${mean_delays[$i]}

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
    wait
done

# Try different time_pull values
lambda_loop=$initial_lambda_loop
lambda_drop=$initial_lambda_drop
lambda_payload=$initial_lambda_payload
path_length=$initial_path_length
lambda_loop_mix=$initial_lambda_loop_mix
mean_delay=$initial_mean_delay
time_pull=$initial_time_pull
max_retrieve=$initial_max_retrieve
pad_length=$initial_pad_length

time_pulls=(0.01 0.05 0.1 0.5 1 2)

mkdir -p metrics/time_pull

for i in "${!time_pulls[@]}"; do
    time_pull=${time_pulls[$i]}

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
    wait
done

