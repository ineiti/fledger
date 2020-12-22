use ext_interface::Network;use ext_interface::Storage;
use super::config::NodeInfo;

// use super::config::parse_config;
use super::ext_interface;
use super::state::Logs;

/// The node structure holds it all together. It is the main structure of the project.
pub struct Server {
    info: NodeInfo,
    state: Logs,
    network: Box<dyn Network>,
    storage: Box<dyn Storage>,
}

// pub fn StartNode(net: impl Network, st: impl Storage) -> Result<Server, &'static str> {
//     let config = st.load(&"nodeConfig")?;
//     let toml_config = match parse_config(config){
//         Ok(i) => i,
//         Err(err) => return Err(err.to_string().as_str())
//     };
//     Ok(Server{
//         info: toml_config.our_node,
//     })
// }
