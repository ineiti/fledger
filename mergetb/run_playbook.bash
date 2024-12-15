#!/bin/bash

directory_path="/home/dcog/configs_dir"

for file in "$directory_path"/*; do
  file_name=$(basename "$file")
  base_name="${file_name%.yaml}"
  
    ansible-playbook -i inventory.ini data_collection.yml --extra-vars "config_file_name=$base_name retry=0 path_len=2"
    # ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=2"

done


    # ansible-playbook -i inventory.ini data_collection.yml --extra-vars "loopix_config_name=loopix_core_config_lambda_loop_0.yaml retry=0 path_len=2"
