use super::config::*;

/// V0.1 states

/// Logs is the first structure being held by the server node.
/// It will be replaced by the Accounts, then the
pub struct Logs {
    broadcast: Box<[LogConnectivity]>
}

pub struct LogConnectivity {
    time: u32,
    connectivities: Box<[Connectivity]>,
}

pub struct Connectivity {
    source: NodeInfo,
    connections: Box<[Connection]>,
}

/// Represents one connection from a node to another.
pub struct Connection {
    with: NodeInfo,
    /// ping-time in miliseconds. If not reachable, -1.
    ping: i32,
}

// use super::types::*;
// use std::collections::HashMap;
// /// The global state that all nodes agreed upon.
// pub struct GlobalState {
//     accounts: HashMap<U256, Account>
// }
//
// // An account holds some type of coins.
// // In the beginning there will be only one type of coins, but probably later different types for
// // different actions like memory storage, calculation, bandwidth, crypto methods, or other things
// // might be created.
// pub struct Account {
//     address: U256,
//     coin_type: U256,
//     amount: BigInt,
// }
//
// // Pre-block implementation, using a map of changes that will be applied to the Current structure.
// pub struct Changes {
//     accounts: Box<[Account]>,
// }
//
// // The global state of the system with regards to accounts.
// pub struct Current {
//     accounts: HashMap<U256, Account>,
// }
//
// // impl Current {
// //     /// Creates a new Current state by going through all proposed changes.
// //     /// If any of the proposed changes has an unmatching address/coin_type, the
// //     /// method will return an error.
// //     fn apply_changes(self, changes: Changes) -> Result<Current, &'static str> {
// //         for account in Vec::from(changes.accounts) {
// //             match self.accounts.get(&account.address) {
// //                 Some(&acc) => {
// //                     if !acc.coin_type.eq(&account.coin_type) {
// //                         return Err(&"Cannot add coins of different type");
// //                     };
// //                     Account {
// //                         amount: acc.amount.add(account.amount),
// //                         ..account
// //                     }
// //                 }
// //                 None => account,
// //             };
// //             self.accounts.insert(account.address, account);
// //         }
// //         return Ok(self);
// //     }
// // }
