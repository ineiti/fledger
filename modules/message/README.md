# Message modules for Fledger

The message modules add a messaging system and dependencies between
the different modules.
While the raw modules are as independant as possible between each other,
the message modules can react to the messages of the other modules.
Each module can send messages that are broadcast to all other modules.

In addition to the message passing, there is a `tick` method that is called
in regular intervals.

Two different types of messages exist:
- `Intern` messages are only propagated between the modules
- `Node2Node` messages are sent to other nodes