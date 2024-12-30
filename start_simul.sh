make kill

PATH_LEN=$1
N_CLIENTS=$2
RETRY=$3
DUPLICATES=$4
TOKEN=$5
if ! [[ "$RETRY" =~ ^[0-9]+$ ]]; then
  echo "Usage: $0 path_len retry"
  echo "retry must be an integer"
  exit 1
fi

(cd cli/fledger && cargo build -r)
(cd cli/flsignal && cargo build -r)

NODES=$(( ( $PATH_LEN + $N_CLIENTS ) + $PATH_LEN * ($PATH_LEN - 1) ))
SIMUL=simul/
rm -rf $SIMUL
mkdir -p $SIMUL

./target-common/release/flsignal |& ts "Signal " &

for NODE in $( seq $NODES ); do
  NAME="NODE_$(printf "%02d" $NODE)"
  echo "Starting node $NAME"
  CONFIG="$SIMUL$NAME/"
  VERBOSITY="-vvv"
  DUPLICATES_ARG="--duplicates $DUPLICATES"
  PATH_LEN_ARG=""
  RETRY_ARG=""
  if [ $NODE = "1" ]; then
    PATH_LEN_ARG="--path-len $PATH_LEN"
  fi
  if [ "$RETRY" -gt 0 ]; then
    RETRY_ARG="--retry $RETRY"
  fi
  START_TIME="--start_loopix_time 20"
  N_CLIENTS_ARG="--n-clients $N_CLIENTS"
  SAVE_NEW_METRICS_FILE="--save_new_metrics_file"
  TOKEN_ARG="--token $TOKEN"
  mkdir -p $CONFIG
  cp "loopix_core_config.yaml" $CONFIG
  LOG_FILE="$CONFIG/log.txt"
  RUST_BACKTRACE=full ./target-common/release/fledger --config $CONFIG $DUPLICATES_ARG $START_TIME $SAVE_NEW_METRICS_FILE --name $NAME $VERBOSITY -s ws://localhost:8765 $PATH_LEN_ARG $RETRY_ARG $N_CLIENTS_ARG $TOKEN_ARG |& ts "$NAME" >> "$LOG_FILE" 2>&1 &
done

sleep 360

make kill