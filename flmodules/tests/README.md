# Tests of modules

## Loopix

This is a simple test which sets up:
- a node with a web-proxy for starting the request
- a node with the web-proxy to receive the request and send it to the internet
- a list of loopix nodes in between

You can run it with:

```bash
cargo test --test loopix -- --nocapture
```

Once everything is set up, it should print the content of the fledg.re webpage.