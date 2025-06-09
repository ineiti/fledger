use crate::Fledger;
use metrics::absolute_counter;

#[derive(Clone)]
pub struct SimulationRealm {}

impl SimulationRealm {
    pub async fn run_dht_join_realm(mut f: Fledger) -> anyhow::Result<()> {
        f.loop_node(crate::FledgerState::DHTAvailable).await?;
        log::info!("SIMULATION END");

        absolute_counter!("fledger_realms_total", 1);

        f.loop_node(crate::FledgerState::Forever).await?;
        return Ok(());
    }
}
