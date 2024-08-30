#[cfg(feature = "wasm")]
mod wasm;
#[cfg(feature = "wasm")]
pub use wasm::*;

#[cfg(not(feature = "wasm"))]
mod libc;
#[cfg(not(feature = "wasm"))]
pub use libc::*;
