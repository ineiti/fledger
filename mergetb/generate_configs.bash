#!/bin/bash

lambda_loop=10.0
lambda_drop=10.0
lambda_payload=10.0
path_length=2
mean_delay=2
lambda_loop_mix=10.0
time_pull=1
max_retrieve=1
pad_length=150

save_config() {
    local variable_name=$1
    local index=$2
    local config_content=$3

    local dir_name="configs_dir"
    mkdir -p "$dir_name"

    local file_name="$dir_name/loopix_core_config_${variable_name}_${index}.yaml"
    echo "Saving to file: $file_name"

    echo "$config_content" > "$file_name"
}

lambda_loops=(1 5 10 15 20 25 30 35 40 45 50 55 60)
max_retrieves=(1 3 5 7 9)
mean_delays=(0.02 0.2 1 2 5 10 20 200)
time_pulls=(0.1 0.5 1 2 5 10)
path_lengths=(2 3 4 5 6 7 8)

for i in "${!lambda_loops[@]}"; do
    lambda_loop=${lambda_loops[$i]}
    config_content="---
lambda_loop: $lambda_loop
lambda_drop: $lambda_drop
lambda_payload: $lambda_payload
path_length: $path_length
mean_delay: $mean_delay
lambda_loop_mix: $lambda_loop_mix
time_pull: $time_pull
max_retrieve: $max_retrieve
pad_length: $pad_length"

    save_config "lambda_loop" $i "$config_content"
done

for i in "${!max_retrieves[@]}"; do
    max_retrieve=${max_retrieves[$i]}
    config_content="---
lambda_loop: $lambda_loop
lambda_drop: $lambda_drop
lambda_payload: $lambda_payload
path_length: $path_length
mean_delay: $mean_delay
lambda_loop_mix: $lambda_loop_mix
time_pull: $time_pull
max_retrieve: $max_retrieve
pad_length: $pad_length"

    save_config "max_retrieve" $i "$config_content"
done

for i in "${!mean_delays[@]}"; do
    mean_delay=${mean_delays[$i]}
    config_content="---
lambda_loop: $lambda_loop
lambda_drop: $lambda_drop
lambda_payload: $lambda_payload
path_length: $path_length
mean_delay: $mean_delay
lambda_loop_mix: $lambda_loop_mix
time_pull: $time_pull
max_retrieve: $max_retrieve
pad_length: $pad_length"

    save_config "mean_delay" $i "$config_content"
done

for i in "${!time_pulls[@]}"; do
    time_pull=${time_pulls[$i]}
    config_content="---
lambda_loop: $lambda_loop
lambda_drop: $lambda_drop
lambda_payload: $lambda_payload
path_length: $path_length
mean_delay: $mean_delay
lambda_loop_mix: $lambda_loop_mix
time_pull: $time_pull
max_retrieve: $max_retrieve
pad_length: $pad_length"

    save_config "time_pull" $i "$config_content"
done

for i in "${!path_lengths[@]}"; do
    path_length=${path_lengths[$i]}
    config_content="---
lambda_loop: $lambda_loop
lambda_drop: $lambda_drop
lambda_payload: $lambda_payload
path_length: $path_length
mean_delay: $mean_delay
lambda_loop_mix: $lambda_loop_mix
time_pull: $time_pull
max_retrieve: $max_retrieve
pad_length: $pad_length"

    save_config "path_length" $i "$config_content"
done
