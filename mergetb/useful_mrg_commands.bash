# mrg xdc scp upload -x fledgerxdc.dcog ./target-common/release/fledger node-2:/home/dcog/fledger


# ssh into mergetb 
mrg xdc ssh fledgerxdc.dcog 
mrg xdc ssh -x fledgerxdc.dcog node-1

# ping all nodes
ansible -i inventory.ini ALL_NODES -m ping

# generate inventory and send it to xdc
mrg show materialization tryagain.fledgerfirst.dcog > materialization.txt
python3 generate_inventory.py

# upload files to xdc
mrg xdc scp upload inventory.ini fledgerxdc2.dcog:/home/dcog
mrg xdc scp upload playbook.yml fledgerxdc2.dcog:/home/dcog
mrg xdc scp upload install_docker.yml fledgerxdc2.dcog:/home/dcog
mrg xdc scp upload delete_docker.yml fledgerxdc2.dcog:/home/dcog
mrg xdc scp upload stop_containers.yml fledgerxdc2.dcog:/home/dcog
mrg xdc scp upload ../../loopix_core_config.yaml fledgerxdc2.dcog:/home/dcog



# # build and upload binaries
# (cd ../cli/fledger && cargo build -r)
# (cd ../cli/flsignal && cargo build -r)
# mrg xdc scp upload ../target-common/release/flsignal fledgerxdc.dcog:/home/dcog
# mrg xdc scp upload ../target-common/release/fledger fledgerxdc.dcog:/home/dcog

# run experiment
nohup ansible-playbook -i inventory.ini delete_docker.yml > playbook.log 2>&1
nohup ./get_simul_data.sh > logs.txt 2>&1 &
nohup ansible-playbook -i inventory.ini playbook.yml --extra-vars "retry=0 path_len=3 variable=lambda_payload index=0" > logs.txt 2>&1 &


# compress metrics data
tar -cvf - metrics/grid_search | pigz -p $(nproc) > metrics_grid_search.tar.gz




