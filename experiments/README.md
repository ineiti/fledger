# MergeTB experiments

## Generalities

Those experiments are meant to be run on sphere.

Each is composed of:

- A README (`README.md`)
- A mergeTB model (`model.py`)
- An ansible playbook (`playbook.yaml`)
- The ansible inventory (`hosts`)
- Oneshot systemd services to run the actual nodes

And sometimes:

- A plantUML sequence diagram of the experiment (`explanation.puml`, `explanation.png`)
- A plantUML network diagram (`network.puml`, `network.png`)

The services can either be ran synchronously (ansible blocks) or asynchronously
(ansible starts the node and goes to the next task).
Check `exp1-dummy` for an example of both
(fledger-send is blocking and fledger-recv is non-blocking).

## Running experiments

### Setup mergeTB

Read the documentation and do the hello world.

Then, create one XDC to be used for all experiments.
Use this configuration (read the documentation before using it):

```bash
Host sphere
 Hostname jump.sphere-testbed.net
 Port 2022
 User YOUR_USER
 IdentityFile ~/.ssh/merge_key
 ServerAliveInterval 30

Host sphere-fledger
 ProxyJump sphere
 IdentityFile ~/.ssh/merge_key
 User YOUR_USER
 Hostname SSH_NAME_FROM_SPHERE
```

Don't forget to replace the values for `YOUR_USER` and `SSH_NAME_FROM_SPHERE`.

On the XDC, install ansible (use the ansible/ansible apt ppa!).

Configure ansible with this `~/.ansible.cfg` from the documentation :

```bash
[defaults]
# don't check experiment node keys, if this is not set, you will have to
# explicitly accept the SSH key for each experiment node you run Ansible
# against
host_key_checking = False

# configure up to 5 hosts in parallel
forks = 5

# tmp directory on the local non-shared filesystem. Useful when running ansible
# on multiple separate XDCs
local_tmp = /tmp/ansible/tmp

[ssh_connection]

# connection optimization that increases speed significantly
pipelining = True

# control socket directory on the local non-shared filesystem. Useful when
# running ansible on multiple separate XDCs
control_path_dir = /tmp/ansible/cp
```

### Activate the nodes and attach the XDC

Use the model.py to create the reservation and the activation (materialization).
Attach your XDC to the materialization.

### Deploy and run

- Go to the root of the project on your local machine.
- Activate devbox with `devbox shell`.
- Run `make deploy_experiments_binaries` to
compile and upload the binaries to the XDC.
- Run `make deploy_experiments` to upload the experiment files as well.
- SSH into your XDC.
- Go to the experiment's folder.
- Run `ansible-playbook -i hosts playbook.yaml`
