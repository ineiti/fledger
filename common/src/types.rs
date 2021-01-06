use core::fmt;

use rand::random;use serde::{Serialize, Deserialize};

/// Nicely formatted 256 bit structure
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
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

impl U256 {
    pub fn rnd() -> U256 {
        U256{0: random()}
    }
}
