pub type U256 = [u8; 32];

// // Probably using the num crate here will fail if compiled to wasm, so implement our own bigint...
// pub struct BigInt(Box<[u8]>);
//
// impl BigInt {
//     pub fn add(&self, other: BigInt) -> BigInt {
//         return *self;
//     }
// }
