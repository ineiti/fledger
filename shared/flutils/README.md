# Utils for the Fledger system

Some common utils used by the Fledger system.
The most important is the `Broker` structure that is used throughout
the code to link the different parts together.

## Broker

I wanted to create a common code for both the libc- and wasm-implementation for
Fledger.
Unfortunately it is difficult by the fact that libc allows to use threads
(and sometimes needs them), so some structures need to have the `Send` and `Sync`
traits.
But these traits are not available for all necessary websys-modules!
So I came up with the idea of linking all modules using a `Broker` system.

In short, all input and output for a module are defined as messages.
Then each module handles incoming messages and produces outgoing messages.
Modules can be linked together by defining `Translators` that take messages
from one module and translate them into messages for the other module.
