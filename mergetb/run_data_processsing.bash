#!/bin/bash

DATA_DIR=$1
PATH_LENGTH=$2
N_CLIENTS=$3
DURATION=$4

if [ -z "$DATA_DIR" ] || [ -z "$PATH_LENGTH" ] || [ -z "$N_CLIENTS" ] || [ -z "$DURATION" ]; then
  echo "Usage: $0 <data_dir> <path_length> <n_clients> <duration>"
  exit 1
fi

source ../venv/bin/activate

python3 extract_raw_data.py "$DATA_DIR" "$PATH_LENGTH" "$N_CLIENTS"

python3 average_data.py "$DATA_DIR" "$DURATION"

python3 plot_data.py "$DATA_DIR" "$PATH_LENGTH" "$DURATION"