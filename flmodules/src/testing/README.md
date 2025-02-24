# Testing Simulators

The different simulators here allow to choose different trade-offs between
simulation precision and speed:

- [network_simul](./network_simul.rs) abstracts away the signalling server, but is a bit faster
- [router_simul](./router_simul.rs) only offers a router to connect to, even faster

The following two simulations might come in handy, but currently they haven't been written yet:
- `signal_simul` is the most precise, but slowest, as it needs to simulat all layers
- `dht_router_simul` useful for modules which only use the dht_router module

Choose wisely your level of abstraction!
And, of course, once your simulation works, but the real test fails, you can always do a better simulation
for the failing tests.
