# Ping example

This example only works as a rust binary.
It is a simple example of how the flnet crate can work and set
up a communication between two endpoints.

You can build it and then run it in different modes: `ping`, `server`, `client`.

## Server Action

Sets up a server and prints the id of the server on the console.
This ID can then be used by the client to connect to the server.
Before it can be run, the signalling server needs to be started locally in
a separate terminal:

```bash
cd ../../cli && cargo run -p flsignal -- -vv
```

Once the signalling server is started, the server can be started:

```bash
cargo run -- --action server
```

## Client Action

Works together with the `Server Action` and also needs the signalling server
to run.
You need to pass the ID of the server to the client:

```bash
cargo run -- --action client --server_id server_id
```

## Ping Action

The ping action shows how to connect to all available nodes.
It requests the list of the nodes every 10 seconds.
Then it sends a 'ping' message to all nodes in the list, except itself.
This allows the endpoints to work without having to know the ID of
the other endpoints.
But it relies on the signalling server to indicate all available nodes.

To run it, a signalling server needs to be started first,
like with the `Server Action`:

```bash
cd ../../cli && cargo run -p flsignal -- -vv
```

Then you must run at least two nodes, but you can run as many as you
like:

```bash
cargo run -- --action ping
```
