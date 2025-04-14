#!/bin/bash

mkdir -p env.systemd

amount=100

for i in $(seq 0 $((amount - 1))); do
  nodename="n${i}"
  envfile="env.systemd/${nodename}.env"

  signalhost="10.0.128.128"

  {
    echo "FLEDGER_CENTRAL_HOST=${signalhost}"
    echo "FLEDGER_NODE_NAME=${nodename}"
  } >"$envfile"
done
