use crate::simulation_chat::simulation::SimulationChat;
use crate::simulation_realm::simulation::SimulationRealm;
use crate::{simulation_dht_target::simulation::SimulationDhtTarget, Fledger};
use clap::{arg, Args, Subcommand};
use flarch::random;
use flarch::tasks::wait_ms;

#[derive(Args, Debug, Clone)]
pub struct SimulationCommand {
    /// Print new messages as they come
    #[arg(long, default_value = "false")]
    pub print_new_messages: bool,

    #[command(subcommand)]
    pub subcommand: SimulationSubcommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum SimulationSubcommand {
    Chat {
        /// Send a simulation_chat message upon node creation
        #[arg(long)]
        send_msg: Option<String>,

        /// Wait for a simulation_chat message with the given body.
        /// log "RECV_CHAT_MSG TRIGGERED" upon message received, at log level info
        #[arg(long)]
        recv_msg: Option<String>,
    },

    DhtJoinRealm {},

    DhtCreateFillersAndTarget {
        #[arg(long)]
        filler_amount: u32,

        #[arg(long)]
        page_size: u32,

        #[arg(long)]
        pages_propagation_delay: u32,

        #[arg(long)]
        connection_delay: u32,

        #[arg(long)]
        experiment_id: u32,
    },

    DhtFetchTarget {
        #[arg(long, default_value = "20000")]
        timeout_ms: u32,

        #[arg(long, default_value = "false")]
        enable_sync: bool,

        #[arg(long)]
        experiment_id: u32,
    },
}

pub struct SimulationHandler {}

impl SimulationHandler {
    pub async fn run(f: Fledger, command: SimulationCommand) -> anyhow::Result<()> {
        // wait a random amount of time before running a simulation
        // to avoid overloading the signaling server
        if f.args.bootwait_max != 0 {
            let randtime = random::<u64>() % args.bootwait_max;
            log::info!("Waiting {}ms before running this node...", randtime);
            wait_ms(randtime).await;
        }

        let loop_delay = f.args.loop_delay;
        match command.subcommand.clone() {
            SimulationSubcommand::Chat { send_msg, recv_msg } => {
                SimulationChat::run_chat(f, command, send_msg, recv_msg).await
            }
            SimulationSubcommand::DhtJoinRealm {} => SimulationRealm::run_dht_join_realm(f).await,
            SimulationSubcommand::DhtCreateFillersAndTarget {
                filler_amount,
                page_size,
                pages_propagation_delay,
                connection_delay,
                experiment_id,
            } => {
                SimulationDhtTarget::run_create_fillers_and_target(
                    f,
                    filler_amount,
                    page_size,
                    pages_propagation_delay,
                    connection_delay,
                    experiment_id,
                )
                .await
            }
            SimulationSubcommand::DhtFetchTarget {
                timeout_ms,
                enable_sync,
                experiment_id,
            } => {
                SimulationDhtTarget::fetch_target(
                    f,
                    loop_delay,
                    enable_sync,
                    timeout_ms,
                    experiment_id,
                )
                .await
            }
        }
    }
}
