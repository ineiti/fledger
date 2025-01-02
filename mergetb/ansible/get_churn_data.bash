#!/bin/bash

if [ $# -lt 1 ]; then
    echo "Usage: $0 <token>"
    exit 1
fi

token=$1

initial_path_length=3

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

    ansible-playbook -i inventory.ini playbook_churn.yml --extra-vars "retry=$retry path_len=$initial_path_length n_clients=3 duplicates=1 token=$token variable=retry index=$i"
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

    ansible-playbook -i inventory.ini playbook_churn.yml --extra-vars "retry=0 path_len=$initial_path_length n_clients=3 duplicates=$duplicates token=$token variable=duplicates index=$i"
    wait
    ansible-playbook -i inventory.ini stop_containers.yml 
    wait
    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait
done


# retry and duplicates
mkdir -p metrics/retry_duplicates

duplicates_values=(2 3 4 5)
retry_values=(1 2 3 4)
retry_duplicates="{"
for i in "${!duplicates_values[@]}"; do
    for j in "${!retry_values[@]}"; do
        duplicates=${duplicates_values[$i]}
        retry=${retry_values[$j]}
        index=$((i * j + j))
        retry_duplicates+="\"$index\": {\"duplicates\": $duplicates, \"retry\": $retry},"
    done
done
retry_duplicates="${retry_duplicates%,}}"
echo -e "$retry_duplicates" > metrics/retry_duplicates/retry_duplicates.json

for i in "${!duplicates_values[@]}"; do
    for j in "${!retry_values[@]}"; do
        duplicates=${duplicates_values[$i]}
        retry=${retry_values[$j]}
        index=$((i * j + j))

        ansible-playbook -i inventory.ini playbook_churn.yml --extra-vars "retry=$retry path_len=$initial_path_length n_clients=3 duplicates=$duplicates token=$token variable=retry_duplicates index=$index"
        wait
        ansible-playbook -i inventory.ini stop_containers.yml 
        wait
        ansible-playbook -i inventory.ini delete_only_metrics.yml
        wait
    done
done

# Control run
time_pulls=(0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7 0.7)
mkdir -p metrics/control

for i in "${!time_pulls[@]}"; do
    time_pull=${time_pulls[$i]}

    ansible-playbook -i inventory.ini playbook_churn.yml --extra-vars "retry=1 path_len=$initial_path_length n_clients=3 duplicates=2 token=$token variable=control index=$i"
    wait
    ansible-playbook -i inventory.ini stop_containers.yml 
    wait
    ansible-playbook -i inventory.ini delete_only_metrics.yml
    wait
done
