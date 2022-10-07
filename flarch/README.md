# FlArch

The Fledger Arch module holds common methods that are used by libc and wasm
implementation.
The following methods / structures are available:
- `DataStorage` allows to store key/value pairs in a file / localStorage
- `tasks::*` various useful tools:
  - `now() -> f64` - returns the current timestamp in milliseconds as f64
  - `spawn_local<F: Future<Output = ()> + 'static>(f: F)` - spawns a future locally
  - `wait_ms(ms: u32)` - async wait in milliseconds

By default the crate compiles for `libc`.

## Features

- `wasm` compiles for the wasm target
- `node` compiles for the node target