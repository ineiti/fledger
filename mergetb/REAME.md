# MergeTB

This folder contains the scripts used to run experiments and process data.

The ansible folder contains the scripts used to run the simulations with the MergeTB XDC. To set up experiments as well as the XDC please refer to the Sphere Research Infrastructure documentation.

Firstly with MergeTB, an experiment needs to be reserved and activated. The configuration for the experiment can be found in experiment.py

Once the experiment is activated, and ansible is isntalled on the XDC, the following playbooks can be used to run the simulations:

1. `playbook.yml` runs the simulation without any churn for 300 seconds
2. `playbook_churn.yml` kills specificied amount of nodes at the start of the simulation

The playbooks require inventory.ini to be present in the folder. This file can be generated using ansible/generate_inventory.py according to the mergetb reservation.

To collect data with different parameters, the following scripts can be used (they run the playbooks with different parameters and collect the data in the xdc)

1. ./ansible/get_simul_data.bash: runs the simulation with different configurations of means delays and lambda paramteres
2. ./ansible/max_retreive_time_pull_grid_search.bash: runs a grid search for the max retrieve time for the pull parameters
3. ./ansible/get_churn_data.bash: runs playbook_churn with different values of retry and duplicate mechanisms
4. ./ansible/find_mu.bash: runs the playbook.yml with different values of mean number of messages per second at a client

To process the data from the simulations, the following scripts can be used respectively:
1. ./run_data_processing.bash
2. ./run_grid_search_data_processing.bash
3. ./run_churn_data_processing.bash
4. ./find_mu.bash

Note that the data processing scripts require the data to be save in the <data_dir>/raw_data folder.

A venv is also required to run the data processing scripts (at the path ../venv), the requirements can be found in ../requirements.txt

# Example 

First generate the inventory.ini for an experiment with path length 3 and 3 clients and reservation name (on mergetb) "test":
```bash
cd ansible
python3 generate_inventory.py 3 3 test
```

Upload all the required files to the XDC. Example commands can be found in useful_mrg_commands.bash.

Make sure the nodes have docker installed (install_docker.yml). (after this installation the node might neeed a warm reboot)

Run the a with the following command:
```bash
nohup ./get_simul_data.bash <ipinfo.io token> > logs.txt 2>&1 &
```

All data from the the simulation will be saved in the ./metrics directory in the XDC.

Once the data is collected, download this data into ./mean_delay_lambdas/raw_data

The data processing can be run with the following command:
```bash
./run_data_processing.bash mean_delay_lambdas <path_length> <n_clients> <duration>
```
In our case path_length = 3, n_clients = 3 and duration = 300.

This will extract data from the metrics files and create two files in ./mean_delay_lambdas:
1. raw_data.json: the same data from the metrics files in json format
2. average_data.json: the data averaged for each run

The script will also generate a plots directory in ./mean_delay_lambdas/plots where are all the related plots will be saved.

## Other playbooks
 Other playbooks in the ansible folder are used to delete the data directory in the nodes, delete the docker image, stop container etc.





