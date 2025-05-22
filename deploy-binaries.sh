#!/bin/bash

if test "$#" -ne 0; then
  echo "usage: $0"
  exit 1
fi

if ! test -f "target-common/x86_64-unknown-linux-musl/release/flsignal"; then
  echo "ERROR: you did not compile flsignal with musl. The experiment will not work."
  echo "Aborting..."
  exit 1
fi

if ! test -f "target-common/x86_64-unknown-linux-musl/release/fledger"; then
  echo "ERROR: you did not compile fledger with musl. The experiment will not work."
  echo "Aborting..."
  exit 1
fi

echo "Uploading fledger and flsignal binaries..."
scp \
  target-common/x86_64-unknown-linux-musl/release/fledger \
  target-common/x86_64-unknown-linux-musl/release/flsignal \
  sphere-fledger:/usr/share/caddy/

# echo "Pushing fledger and flsignal binaries to github..."
# cp \
#   target-common/x86_64-unknown-linux-musl/release/fledger \
#   target-common/x86_64-unknown-linux-musl/release/flsignal \
#   ../fledger-binaries/
# cd ../fledger-binaries/ || exit 1
# git add . || exit 1
# git commit -m "deploy" || exit 1
# git push || exit 1
