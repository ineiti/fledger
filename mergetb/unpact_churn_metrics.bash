#!/bin/bash

DATA_DIR=$1

if [ -z "$DATA_DIR" ]; then
  echo "Usage: $0 <data-directory>"
  exit 1
fi

for main_dir in "$DATA_DIR"/*; do
  if [ -d "$main_dir" ]; then
    echo "Entering metrics directory: $main_dir"
    
    for sub_dir in "$main_dir"/*; do
      if [ -d "$sub_dir" ]; then
        echo "Entering variable directory: $sub_dir"

        for run_dir in "$sub_dir"/*; do
          if [ -d "$run_dir" ]; then
            echo "Entering run directory: $run_dir"

            for node_dir in "$run_dir"/*; do
              if [ -d "$node_dir" ]; then
                echo "Looking in node directory: $node_dir"

                tar_file="$node_dir/data.tar.gz"
                if [ -f "$tar_file" ]; then
                  echo "Extracting $tar_file in $node_dir"
                  tar -xzf "$tar_file" -C "$node_dir" || echo "Failed to extract $tar_file"
                else
                  echo "No data.tar.gz found in $node_dir"
                fi
              fi
            done
          fi
        done
      fi
    done
  fi
done
