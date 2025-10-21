Following https://keepachangelog.com/en/1.1.0/ and using
- `Added` for new features.
- `Changed` for changes in existing functionality.
- `Deprecated` for soon-to-be removed features.
- `Removed` for now removed features.
- `Fixed` for any bug fixes.
- `Security` in case of vulnerabilities.

See [WIP.md](./WIP.md) for current work and bugs to fix

## [0.9.3] - Pending

### Added

- flbrowser
  - add editing of new pages
  - show cuckoo pages on the bottom

### Changed

- webrtc - better error reporting
- added Modules::stable for a default of all modules which should be used
- Using Node::start_network
- adding TimerBroker to network_start
- flarch::Browser got macros for handling forward messages
- WebRTCConnInput, WebRTCConnOutput to WebRTCConnIn, WebRTCConnOut
- Adding NetworkMessage to NetworkOut
- removed possibility to give configuration on the URL
- handle connections and disconnections better
- created different templates
- updated flmodules to match templates:
  - dht_router
  - dht_storage
  - gossip_events

### Fixed

- enforce single tab and only one connection per node-ID
- flmodules::network - add chunking of big messages
- flmodules::realmstorage - avoid cycling of flos when
flo A and B replace each other
- webrtc sends first message before connection setup message

## [0.9.2] - 2025-05-11

### Added
- flbrowser - page edits, cuckoos, parents, children

### Changed
- flbrowser - re-arranging files and modularizing a bit more
- DHT_router
  - change broadcast:
    - add NeighborMessage which is not forwarded
    - DHTRouterIn::Broadcast sends to all active nodes a NeighborMessage
    - The answer is also a NeighborMessage
- FLSignal
  - Don't send full list of nodes
- UI chat scrolls to the bottom when showing the first message.
- `RealmView` is now tied to have a `root-page` and a `root-tag`
- added configuration options to `cli/fledger` for local run only

## [0.9.1] - 2025-03-17

### Added

### Changed
- flarch::Broker - replaced `Broker` with `BrokerIO` everywhere
- flmodules::* - unified the use of `broker.rs` / `message.rs` / `core.rs` in all modules
- flcrypto / flmodules::flo - simplified usage of Flos wrt signing
- using `VersionedSerde` for node configurations now
- config files are now stored as `.yaml` instead of `.toml`

### Fixed
- correct loading of old configurations

## [0.9.0] - 2025-02-24

This release is mostly for the start of the student project.
It is a mostly working version of the dht_storage module, but most of the CLI tools
and web-frontend are still missing.

### Added
- flcrypto: for some basic cryptography wrappers around different types of signatures
- flmacro: with macros for simplifying the `Send` / `Sync`
- flmodules::flo handling Fledger Objects
- flmodules::dht_router implements a routing based on Kademlia
- flmodules::dht_storage to store FLOs in a distributed hash table

### Fixed
- reconnections should work better now, both for libc and wasm
- removed mdns calls, so it doesn't flood my home network

### Changed
- re-arranged file names in the flmodules section, to better fit with the `template` module
- changed the names of the networking messages
- added an `Overlay` module to abstract the network handling
- more changes in names of the messages to remove ambiguities
- Renamed `Overlay` to `Router`
  - think how the `Overlay` (should be renamed to `Adapter` or so) can be
  redone. One possibility is to have the network module using a good
  `NetworkMessage` which includes the `NetworkWrapper` and can also be used
  by `Random` and `Loopix`.
      - Question: how to handle special messages then? Like asking to reshuffle
      connections in `random` or accessing the providers in `loopix`?
      - Answer: they can be added as a broker with another message type, which is also
      added to the broker structure
        - add an internal message enum to separate them from the outside messages

## [0.8.0] - 2024-09-09

### Changed
- Updated versions of most dependencies
- use tokio::sync::watch to pass configuration from `Translate` to `Broker`
- re-arranged modules:
  - removed flnet
  - put all arch-dependant code to flarch
  - replaced feature-flags "wasm", "libc", "nosend" with #[cfg(target_family="(wasm|unix)")]
- updated login screen in flbrowser

### Removed
- flnet went into flarch and flmodules

### Added
- Create a template to fill out
- License files
- Devbox
- Create a proxy module that links to the html display
- Added a webproxy module to send GET requests from another node

## [0.7.0] - 2022-08-01

### Added
  - Add background processing of broker messages
  - Re-arranging crates and publish
  - Adding examples

### Changed
  - cleaned up for a release of flnet
      - merged flnet-libc and flnet-wasm into flnet
      - simplified directory hierarchy
  - Moving stuff around to make it easier to understand

## [0.6.0] - 2022-05-01

### Changed
  - Rewrite of networking layer

### Added
  - Use real gossiped decentralized message passing
  - Offer crates for using parts of fledger directly

## [0.5.0] - 2022-03-01

### Changed
  - Rewrote big parts of the library and application to make it more modular

## [0.4.1] - 2021-09-16

### Changed
  - Using thiserror::Error instead of String

## [0.4.0] - 2021-07-27

### Changed
  - Added signature to the connection with the signal-server, thanks to
      Bolton Bailey <bolton.bailey@gmail.com>
    during the IC3 Hackathon

## [0.3.0] - 2021-04-08

### Changed
  - More stable everything
  - Clean up a lot of locking issues
  - Fixing issues

## [0.2.3] - 2021-03-04

### Added
  - Add docker-compose.yaml

### Fix
  - Fix node Running

## [0.2.2] - 2021-03-02

### Added
  - Run some nodes constantly on https://fledg.re to have a minimum consensus

## [0.2.1] - 2021-02-28

### Changed
  - Make the https://web.fledg.re a bit nicer and more automatic

## [0.2] - 2021-02-26

### Added
  - Simple ping test with the nodes
  - CLI node using headless Chrome
  - Have website https://fledg.re running and pointing to an up-to-date Fledger code

## [0.1] - 2021-02-05

### Added
  - add ICE connection through the server
    - use websockets to connect to server
    - implement connection in Node

## [0.0] - 2020-12-xx

### Added
  - start the idea
