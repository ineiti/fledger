# Common modules

The modules in here need to be independant of each other.
The following data structures are common:
- NodeID of type U256 to identify a node
- DataStorage to load and save data - each module can suppose that it can store any
key/value pair without colliding with other modules

Each module should have the following:
- An initializer taking a DataStorage and a ModuleConfiguration
- Handlers for incoming messages which only return an empty `Result`
- All handlers should be `sync` (or perhaps not, if DataStorage needs to be async)
- All handlers should implement 