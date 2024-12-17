#!/bin/bash

if [ $# -ne 3 ]; then
    echo "Usage: $0 <path_length> <dir_name> <number>"
    exit 1
fi

path_length=$1
dir_name=$2
number=$3

mkdir -p "simul_data/${dir_name}/${number}"
cp loopix_core_config.yaml "simul_data/${dir_name}/${number}/loopix_core_config.yaml"
cp logs.txt "simul_data/${dir_name}/${number}/logs.txt"

for n in $(seq -f "%02g" $((path_length*2 + path_length*path_length)))
do
    cp simul/NODE_${n}/loopix_storage.yaml "simul_data/${dir_name}/${number}/loopix_storage_${n}.yaml"
    cp simul/NODE_${n}/metrics.txt "simul_data/${dir_name}/${number}/metrics_${n}.txt"

done
