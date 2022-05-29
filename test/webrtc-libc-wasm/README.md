# Test wasm node

This is only used to test manually the interaction between a wasm
node and a libc node.

To run, type:

```
make
```

Which should start the libc-part to function as a signal server and a
node.
Then it should start the wasm-counterpart using `node`, to connect to
the signal server and the other node.