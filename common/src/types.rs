use core::fmt;
use std::{
    num::ParseIntError,
    sync::{Arc, Mutex},
};

use rand::random;
use serde::{Deserialize, Serialize};
use sha2::digest::{consts::U32, generic_array::GenericArray};

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
    pub fn from_str(s: &str) -> Result<U256, String> {
        if s.len() > 64 {
            return Err("give no more than 64 hex chars".to_string());
        }
        let v: Vec<u8> = (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
            .collect::<Result<Vec<u8>, ParseIntError>>()
            .map_err(|e| e.to_string())?;
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

pub trait DataStorage {
    fn load(&self, key: &str) -> Result<String, String>;

    fn save(&self, key: &str, value: &str) -> Result<(), String>;
}

pub type ProcessCallback = Arc<Mutex<Box<dyn FnMut()>>>;

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
