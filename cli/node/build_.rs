use vergen::{ConstantsFlags, Error, gen};

fn main() -> Result<(), Box<dyn std::error::Error>> {
  // Generate the 'cargo:' instruction output
  gen(ConstantsFlags::all()).map_err(Error::into)
}
