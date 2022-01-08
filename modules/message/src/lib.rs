pub mod module;
pub mod gossip_chat;
pub mod random_connections;
pub mod connections;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
