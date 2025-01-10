#!/bin/bash

if [ $# -lt 1 ]; then
    echo "Usage: $0 <token>"
    exit 1
fi

token=$1

initial_path_length=3

lambda_loop=1.35 
lambda_drop=1.35
lambda_payload=5
path_length=3
mean_delay=100
lambda_loop_mix=1.35
time_pull=1
max_retrieve=5
pad_length=150

    cat <<EOL > loopix_core_config.yaml
---
lambda_loop: $lambda_loop
lambda_drop: $lambda_drop
lambda_payload: $lambda_payload
path_length: $initial_path_length
mean_delay: $mean_delay
lambda_loop_mix: $lambda_loop_mix
time_pull: $time_pull
max_retrieve: $max_retrieve
pad_length: $pad_length
EOL

# Try retry values
mkdir -p metrics/retry

retry_values=(0 1 2 3 4)

retry_json="{"
for i in "${!retry_values[@]}"; do
    retry=${retry_values[$i]}
    retry_json+="\"$i\": $retry,"
done
retry_json="${retry_json%,}}"
echo -e "$retry_json" > metrics/retry/retry.json

for i in "${!retry_values[@]}"; do
    retry=${retry_values[$i]}

    # no churn
    ansible-playbook -i inventory.ini playbook_churn.yml --extra-vars "retry=$retry path_len=$initial_path_length n_clients=3 duplicates=1 token=$token variable=retry index=${i}_0"
    wait

    ansible-playbook -i inventory.ini stop_containers.yml 
    wait

    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait

    # kill first mixnode
    ansible-playbook -i inventory.ini playbook_churn.yml --extra-vars "stop_first_mixnode=true retry=$retry path_len=$initial_path_length n_clients=3 duplicates=1 token=$token variable=retry index=${i}_1"
    wait

    ansible-playbook -i inventory.ini stop_containers.yml 
    wait

    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait

    # kill two mixnodes
    ansible-playbook -i inventory.ini playbook_churn.yml --extra-vars "stop_first_mixnode=true stop_second_mixnode=true retry=$retry path_len=$initial_path_length n_clients=3 duplicates=1 token=$token variable=retry index=${i}_2"
    wait

    ansible-playbook -i inventory.ini stop_containers.yml 
    wait

    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait

    # kill three mixnodes
    ansible-playbook -i inventory.ini playbook_churn.yml --extra-vars "stop_first_mixnode=true stop_second_mixnode=true stop_third_mixnode=true retry=$retry path_len=$initial_path_length n_clients=3 duplicates=1 token=$token variable=retry index=${i}_3"
    wait

    ansible-playbook -i inventory.ini stop_containers.yml 
    wait

    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait
done


# Try duplicates values
mkdir -p metrics/duplicates

duplicates_values=(1 2 3 4 5)
duplicates_json="{"
for i in "${!duplicates_values[@]}"; do
    duplicates=${duplicates_values[$i]}
    duplicates_json+="\"$i\": $duplicates,"
done
duplicates_json="${duplicates_json%,}}"
echo -e "$duplicates_json" > metrics/duplicates/duplicates.json

for i in "${!duplicates_values[@]}"; do
    duplicates=${duplicates_values[$i]}

    # no churn
    ansible-playbook -i inventory.ini playbook_churn.yml --extra-vars "retry=0 path_len=$initial_path_length n_clients=3 duplicates=$duplicates token=$token variable=duplicates index=${i}_0"
    wait

    ansible-playbook -i inventory.ini stop_containers.yml 
    wait

    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait

    # kill first mixnode
    ansible-playbook -i inventory.ini playbook_churn.yml --extra-vars "stop_first_mixnode=true retry=0 path_len=$initial_path_length n_clients=3 duplicates=$duplicates token=$token variable=duplicates index=${i}_1"
    wait

    ansible-playbook -i inventory.ini stop_containers.yml 
    wait

    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait

    # kill two mixnodes
    ansible-playbook -i inventory.ini playbook_churn.yml --extra-vars "stop_first_mixnode=true stop_second_mixnode=true retry=0 path_len=$initial_path_length n_clients=3 duplicates=$duplicates token=$token variable=duplicates index=${i}_2"
    wait

    ansible-playbook -i inventory.ini stop_containers.yml 
    wait

    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait

    # kill three mixnodes
    ansible-playbook -i inventory.ini playbook_churn.yml --extra-vars "stop_first_mixnode=true stop_second_mixnode=true stop_third_mixnode=true retry=0 path_len=$initial_path_length n_clients=3 duplicates=$duplicates token=$token variable=duplicates index=${i}_3"
    wait

    ansible-playbook -i inventory.ini stop_containers.yml 
    wait

    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait
done

