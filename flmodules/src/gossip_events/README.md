# Gossip Events

Sends events between nodes by random gossipping the currently available
events.
The events are stored in chronological order, and when the buffer is full,
the oldes events are discarded.

It uses the `random_connections` module to choose which nodes it exchanges
messages with.