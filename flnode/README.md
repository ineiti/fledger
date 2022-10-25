# Fledger Node Logic

This directory holds the logic for a Fledger Node.
The `Node` structure sets up the different modules from
`flmodules` and connects them with each other.
It also creates a `NodeData` structure that collects
the different statistics from the modules.

## NodeData

In order for `NodeData` to have the latest statistics
from the modules, it uses the `Update` messages from
the modules:
on every `Update` message, the new statistics is copied
to the `NodeData` structure.
This is done by adding a `tap` to the corresponding
module-broker, and then going through all messages
to find `Update`s.
The advantage of this is to have a structure that does
not need to be protected by a Mutex.