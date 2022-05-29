# Fledger CLI

The fledger CLI allows to run a fledger node on any libc-compatible
system supported by rust.
Once started, it will connect to the fledger network signalling server,
and then contact as many nodes as necessary to have a well-connected
network.
The command takes the following arguments:

```
USAGE:
    fledger [OPTIONS]

OPTIONS:
    -c, --config <CONFIG>            Path to the configuration directory [default: ./fledger]
    -h, --help                       Print help information
    -n, --name <NAME>                Set the name of the node - reverts to a random value if not
                                     given
    -u, --uptime-sec <UPTIME_SEC>    Uptime interval - to stress test disconnections
    -V, --version                    Print version information
```

When `fledger` is called for the first time, it creates a directory
called `./fledger` and puts the configuration init.
One of the configuration files contains the private key of the node,
which will probably be used in a future version to protect some more
important part of the system, so keep it secure and don't share it.