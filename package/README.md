# @danu/danu

Browser library for Fledger network, compiled to WebAssembly.

## Installation

For local development:
```bash
npm install file:../package/pkg
```

When published:
```bash
npm install @danu/danu
```

## Usage

```typescript
import init, { FlNode, initialize } from '@danu/danu';

// Initialize WASM module
await init();

// Set up logging and panic hooks
initialize();

// Create a node instance
const node = new FlNode();

// Connect to signal server
await node.connect('wss://signal.fledg.re');

// Get connection status
const status = node.get_connection_status();
console.log(status);

// Get statistics
const stats = node.get_stats();
console.log(stats);

// Set up event listener for asynchronous events
node.set_event_listener((event) => {
  console.log('Event:', event.type, event.data);

  switch (event.type) {
    case 'connecting':
      console.log('Connecting to:', event.data.signal_server);
      break;
    case 'connected':
      console.log('Connected!');
      break;
    case 'message':
      console.log('Message received:', event.data);
      break;
    // ... handle other events
  }
});

// Remove event listener when done
node.remove_event_listener();
```

## Event System

The package emits events for asynchronous operations. Set up a listener to receive them:

```typescript
node.set_event_listener((event: { type: string, data: any }) => {
  // Handle events
});
```

**Common Event Types:**
- `connecting` - Connection attempt started
- `connected` - Successfully connected
- `disconnected` - Connection closed
- `message` - Message received from peer
- `peer_joined` - Peer joined the network
- `peer_left` - Peer left the network
- `error` - An error occurred
```

## Building

```bash
# Development build
make build

# Production build
make build-release

# Clean build artifacts
make clean

# Create npm package
make pack
```

## Requirements

- Rust toolchain
- wasm-pack (`cargo install wasm-pack`)
- Node.js >= 18

## License

MIT OR Apache-2.0
