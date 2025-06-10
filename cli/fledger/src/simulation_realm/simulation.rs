use crate::Fledger;

#[derive(Clone)]
pub struct SimulationRealm {}

impl SimulationRealm {
    pub async fn run_dht_join_realm(mut f: Fledger) -> anyhow::Result<()> {
        f.loop_node(crate::FledgerState::DHTAvailable).await?;
        log::info!("SIMULATION END");

        f.loop_node(crate::FledgerState::Forever).await?;
        Ok(())
    }
}
