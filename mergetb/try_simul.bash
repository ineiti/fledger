#!/bin/bash

echo "Deleting docker containers"

sleep 15

mrg xdc ssh fledgerxdc.dcog << 'EOF'
nohup ansible-playbook -i inventory.ini delete_docker.yml > playbook.log 2>&1 &
EOF

echo "Done deleting docker containers"
