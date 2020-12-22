pub mod node_list;
pub mod config;
pub mod ext_interface;
// pub mod server;
// pub mod state;
pub mod types;
pub mod rest;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
