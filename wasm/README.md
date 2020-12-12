# WASM of webrtc test

This is the WASM counterpart of the get-to-know WebRTC for @ineiti...

In its final form of this get-to-know, it should do the following:

1. Announce itself to the server at https://dir.fledger.io
2. Loop every 10 seconds:
  1. Fetch the list of currently active nodes from https://dir.fledger.io
  2. Ping all the nodes
  3. Send a report to https://dir.fledger.io
