use serde::{Serialize, Deserialize};

use crate::nodeids::{NodeID, U256};

type DHTIndex = U256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTNetMessage{
    Input(DHTInput),
    Output(DHTOutput)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTInput{
    Node(NodeID, MessageNode)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTOutput{
    Node(NodeID, MessageNode)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageNode{
    Other(String, String),

}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OtherMessage{
    pub module: String,
    pub msg: String,
}