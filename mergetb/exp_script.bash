# mrg xdc scp upload -x fledgerxdc.dcog ./target-common/release/fledger node-2:/home/dcog/fledger

# ansible -i inventory.ini ALL_NODES -m ping

# mrg xdc ssh -x fledgerxdc.dcog node-1


# generate inventory and send it to xdc
mrg show materialization tryagain.fledgerfirst.dcog > materialization.txt
# mrg show materialization twonode.fledgerfirst.dcog > materialization_twonode.txt
python3 generate_inventory.py
mrg xdc scp upload inventory.ini fledgerxdc.dcog:/home/dcog
mrg xdc scp upload inventory_copy.ini fledgerxdc.dcog:/home/dcog
mrg xdc scp upload playbook.yml fledgerxdc.dcog:/home/dcog
mrg xdc scp upload playbook_copy.yml fledgerxdc.dcog:/home/dcog
mrg xdc scp upload install_docker.yml fledgerxdc.dcog:/home/dcog
mrg xdc scp upload delete_docker.yml fledgerxdc.dcog:/home/dcog

nohup ansible-playbook -i inventory.ini delete_docker.yml > playbook.log 2>&1
nohup ./get_simul_data.sh > logs.txt 2>&1 &

# # build and upload binaries
# (cd ../cli/fledger && cargo build -r)
# (cd ../cli/flsignal && cargo build -r)
# mrg xdc scp upload ../target-common/release/flsignal fledgerxdc.dcog:/home/dcog
# mrg xdc scp upload ../target-common/release/fledger fledgerxdc.dcog:/home/dcog

mrg xdc ssh fledgerxdc.dcog 



