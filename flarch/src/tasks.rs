#[cfg(target_family="wasm")]
mod wasm;
#[cfg(target_family="wasm")]
pub use wasm::*;

#[cfg(target_family="unix")]
mod libc;
#[cfg(target_family="unix")]
pub use libc::*;
