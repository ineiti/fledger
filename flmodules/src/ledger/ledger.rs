use bytes::Bytes;
use flarch::nodeids::{NodeID, U256};

pub struct LedgerInstance {
    pub epoch: u64,
    pub index: u64,
    pub merkle_tree: String,
    pub dag: DAG,
    tx_cache: Vec<Transaction>,
}

impl LedgerInstance {
    pub fn new() -> Self {
        Self {
            epoch: 0,
            index: 0,
            merkle_tree: "".into(),
            dag: DAG::new(),
            tx_cache: vec![],
        }
    }

    pub fn add_transaction(&mut self, tx: Transaction) -> TransactionID {
        self.tx_cache.push(tx);
        todo!()
    }

    pub fn commit(&mut self) -> Vertex {
        self.dag.commit(self.tx_cache.drain(0..).collect())
    }

    pub fn add_vertex(&mut self, vx: Vertex) {
        for tx in self.dag.add_vertex(vx) {}
    }
}

pub struct DAG {}

impl DAG {
    pub fn new() -> Self {
        Self {}
    }

    pub fn commit(&mut self, txs: Vec<Transaction>) -> Vertex {
        todo!()
    }

    pub fn add_vertex(&mut self, vx: Vertex) -> Vec<Transaction> {
        todo!()
    }
}

pub struct Vertex {
    owner: NodeID,
    backlinks: Vec<VertexID>,
    transactions: Vec<Transaction>,
    signature: Bytes,
    // The ID of the block is its hash, and this is a cache for
    // premature optimization.
    cache_id: Option<VertexID>,
}

pub type VertexID = U256;

pub struct Transaction {}

pub type TransactionID = U256;
