# Web interface for Fledger

This crate holds a web-interface for fledger, to interact with the other nodes.
It depends on the `common` crate, so it directly updates in sync with the `flnode` cli.

The latest version is always available at https://web.fledg.re

To run it locally, you need to have the following installed:
- `rust` >= 1.51.0
- `wasm-pack` >= 0.10.0
- `npm` >= 7.10.0

Once this is installed, you can run it like this:

```bash
make build serve
```

This will build a version that connects to the nodes available on fledg.re.
If you want to run it for local nodes, you have to run it with

```bash
make build_local serve
```

But this means that you also need to have a local `signal` and eventually one or two `flnode` running.

# HTML / CSS

