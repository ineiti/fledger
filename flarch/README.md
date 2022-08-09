# FlArch

The Fledger Arch module holds common methods that are used by libc and wasm
implementation.
The following methods / structures are available:
- `DataStorage` allows to store key/value pairs in a file / localStorage
- `tasks::*` various useful tools:
  - `now() -> f64` - returns the current timestamp in milliseconds as f64
  - `block_on<F: Future<Output = ()> + 'static + Send>(f: F)` - executes the future in a blocking manner
  - `schedule_repeating<F>(mut cb: F)` - schedules a method repeatedly in 1 second intervals
  - `wait_ms(ms: u32)` - async wait in milliseconds

By default the crate compiles for `libc`.

## Features

- `wasm` compiles for the wasm target
- `node` compiles for the node target