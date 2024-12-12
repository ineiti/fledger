# Check if path_length argument is provided
if [ -z "$1" ]; then
  echo "Usage: $0 <path_length>"
  exit 1
fi

# empty the data directory
rm -rf ./mergetb_data/*

# get number of nodes
path_length=$1
n_nodes=$((path_length * path_length + path_length * 2))

# copy data from each node
for node in $(seq 0 $((n_nodes - 1))); do
   mkdir -p ./mergetb_data/node-${node}
   mrg xdc scp download -x fledgerxdc.dcog -r node-${node}:/home/dcog/data/* ./mergetb_data/node-${node}
done
