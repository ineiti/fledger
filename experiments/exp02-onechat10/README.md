# Exp2 dummy10

Run 10 nodes, fully interconnected.

Each node broadcasts a message (destined to a single node),
and waits for a message from one specific other node.

The playbook succeeds if all nodes receive their message,
and fails if the timeout of 120s is reached.
