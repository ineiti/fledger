#!/bin/bash

mkdir -p env.systemd

amount=50

for i in $(seq 0 $((amount - 1))); do
  nodename="n${i}"
  envfile="env.systemd/${nodename}.env"

  if test "$i" -lt 25; then
    signalhost="10.0.0.128"
  else
    signalhost="10.0.1.128"
  fi

  {
    echo "FLEDGER_CENTRAL_HOST=${signalhost}"
    echo "FLEDGER_NODE_NAME=${nodename}"
  } >"$envfile"
done
