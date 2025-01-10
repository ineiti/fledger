# Ping Module

This module pings other nodes every n seconds to make sure they
are up.
If a node doesn't reply for a given timeframe, it emits a `Failed`
message.

It is based on the `random_connection` module, but should in fact
use the `network` module directly.