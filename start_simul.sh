#!/usr/bin/env bash -e

make kill

PATH_LEN=$1
if [ -z "$PATH_LEN" ]; then
  echo "Usage: $0 path_len"
  exit 1
fi

(cd cli/fledger && cargo build -r)
(cd cli/flsignal && cargo build -r)

NODES=$(( ( $PATH_LEN + 2 ) * $PATH_LEN ))
SIMUL=simul/
rm -rf $SIMUL
mkdir -p $SIMUL

./target-common/release/flsignal -vv |& ts "Signal " &

for NODE in $( seq $NODES ); do
  NAME="NODE_$(printf "%02d" $NODE)"
  echo "Starting node $NAME"
  CONFIG="$SIMUL$NAME/"
  PATH_LEN_ARG=""
  if [ $NODE = "1" ]; then
    PATH_LEN_ARG="--path-len $PATH_LEN"
  fi
  ./target-common/release/fledger --config $CONFIG --name $NAME -vv -s ws://localhost:8765 $PATH_LEN_ARG |& ts "$NAME" &
done