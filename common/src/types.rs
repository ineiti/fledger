use core::fmt;
use futures::Future;
use std::num::ParseIntError;
use thiserror::Error;

use rand::random;
use serde::{Deserialize, Serialize};
use sha2::digest::{consts::U32, generic_array::GenericArray};

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Give no more than 64 hexadecimal characters")]
    HexTooLong,
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    #[error("From the underlying storage: {0}")]
    Underlying(String),
}

/// Nicely formatted 256 bit structure
#[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash)]
pub struct U256([u8; 32]);

impl fmt::Display for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, byte) in self.0.iter().enumerate() {
            if index % 8 == 0 && index > 0 {
                f.write_str("-")?;
            }
            f.write_fmt(format_args!("{:02x}", byte))?;
        }
        Ok(())
    }
}

impl AsRef<[u8]> for U256 {
    fn as_ref(&self) -> &[u8] {
        return &self.0;
    }
}

impl fmt::Debug for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, byte) in self.0.iter().enumerate() {
            if index % 8 == 0 && index > 0 {
                f.write_str("-")?;
            }
            f.write_fmt(format_args!("{:02x}", byte))?;
        }
        Ok(())
    }
}

impl U256 {
    pub fn rnd() -> U256 {
        U256 { 0: random() }
    }

    /// Convert a hexadecimal string to a U256.
    /// If less than 64 characters are given, the U256 is filled from
    /// the left with the remaining u8 initialized to 0.
    /// So
    ///   `U256.from_str("1234") == U256.from_str("123400")`
    /// something
    pub fn from_str(s: &str) -> Result<U256, StorageError> {
        if s.len() > 64 {
            return Err(StorageError::HexTooLong);
        }
        let v: Vec<u8> = (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
            .collect::<Result<Vec<u8>, ParseIntError>>()?;
        let mut u = U256 { 0: [0u8; 32] };
        v.iter().enumerate().for_each(|(i, b)| u.0[i] = *b);
        Ok(u)
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl From<GenericArray<u8, U32>> for U256 {
    fn from(ga: GenericArray<u8, U32>) -> Self {
        let mut u = U256 { 0: [0u8; 32] };
        ga.as_slice()
            .iter()
            .enumerate()
            .for_each(|(i, b)| u.0[i] = *b);
        u
    }
}

impl From<[u8; 32]> for U256 {
    fn from(b: [u8; 32]) -> Self {
        U256 { 0: b }
    }
}

pub trait DataStorage {
    fn load(&self, key: &str) -> Result<String, StorageError>;

    fn save(&mut self, key: &str, value: &str) -> Result<(), StorageError>;
}

#[cfg(target_arch = "wasm32")]
pub fn now() -> f64 {
    use js_sys::Date;
    Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn now() -> f64 {
    use chrono::Utc;
    Utc::now().timestamp_millis() as f64
}

#[cfg(target_arch = "wasm32")]
pub fn block_on<F: Future<Output = ()> + 'static>(f: F) {
    wasm_bindgen_futures::spawn_local(f);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn block_on<F: Future<Output = ()>>(f: F) {
    futures::executor::block_on(f);
}

pub fn type_to_string<T>(_: &T) -> String {
    format!("{}", std::any::type_name::<T>())
}
