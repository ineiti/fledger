(cd cli/fledger && cargo build -r)
(cd cli/flsignal && cargo build -r)

docker build -f Dockerfile.fledger -t deryacog/fledger:latest .

docker build -f Dockerfile.flsignal -t deryacog/flsignal:latest .

docker push deryacog/fledger:latest

docker push deryacog/flsignal:latest