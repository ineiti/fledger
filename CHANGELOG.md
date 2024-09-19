Following https://keepachangelog.com/en/1.1.0/ and using
- Added for new features.
- Changed for changes in existing functionality.
- Deprecated for soon-to-be removed features.
- Removed for now removed features.
- Fixed for any bug fixes.
- Security in case of vulnerabilities.

## [Unreleased]

### Fixed
- reconnections should work better now, both for libc and wasm
- fixed signalling server to correctly close and remove timed-out connections

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