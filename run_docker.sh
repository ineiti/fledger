#!/bin/bash
set -e

apt update -qq
apt install -qq -y rsync > /dev/null
echo
echo "Copying files to local docker for faster compilation. It takes 1-2 minutes, but speeds up compilation to under 1 minute"
rsync -a --exclude target --exclude .git --exclude flbrowser --exclude test --exclude .cargo /mnt/fledger /usr/src
rsync -a /mnt/fledger/.cargo/ /usr/local/cargo/registry
cd /usr/src/fledger
mkdir -p cli/target
cd cli
rsync -a --max-size=100m /mnt/fledger/cli/target/debug-x86/ target/debug/ 
cargo build -j 8 -p flsignal
cargo build -j 8 -p fledger
echo
echo "Copying files back to the mac filesystem."
rsync -a /usr/local/cargo/registry/ /mnt/fledger/.cargo
rsync -a target/debug/ /mnt/fledger/cli/target/debug-x86
