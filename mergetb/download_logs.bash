#!/bin/bash
if [ "$#" -ne 1 ]; then
    echo "Usage: $0 <path_length>"
    exit 1
fi

rm -rf mergetb_data

path_length=$1

n_nodes=$(($path_length * $path_length + $path_length * 2))

for i in $(seq 0 $n_nodes); do
    mkdir -p mergetb_data/node-$i
    mrg xdc scp download -x fledgerxdc.dcog -r node-$i:/home/dcog/data/* mergetb_data/node-$i
done
