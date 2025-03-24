#!/bin/bash

signalhost="10.0.0.1"

mkdir -p env.systemd

messages=()

for i in $(seq 10); do
  message="$(openssl rand -hex 16)"
  messages+=("$message")
done

for i in $(seq 0 9); do
  j=$((("$i" + 2) % 10))
  n=$(("$i" + 97))
  nodename=$(echo $n | awk '{printf("%c",$1)}')
  envfile="env.systemd/${nodename}.env"
  send_msg="${messages[$j]}"
  recv_msg="${messages[$i]}"
  echo "FLEDGER_FLSIGNAL_HOST=${signalhost}" >"$envfile"
  echo "FLEDGER_SEND_MSG=${send_msg}" >>"$envfile"
  echo "FLEDGER_RECV_MSG=${recv_msg}" >>"$envfile"

  echo "[node $nodename]"
  echo "    <- ${recv_msg} [$i]"
  echo "    -> ${send_msg} [$j]"
done
