use crate::{metrics::Metrics, Fledger};
use clap::{arg, Args, Subcommand};
use flarch::{nodeids::U256, tasks::wait_ms};
use flmodules::gossip_events::core::Event;
use metrics::{absolute_counter, increment_counter};

#[derive(Args, Debug, Clone)]
pub struct SimulationCommand {
    /// Print new messages as they come
    #[arg(long, default_value = "false")]
    print_new_messages: bool,

    #[command(subcommand)]
    pub subcommand: SimulationSubcommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum SimulationSubcommand {
    Chat {
        /// Send a chat message upon node creation
        #[arg(long)]
        send_msg: Option<String>,

        /// Wait for a chat message with the given body.
        /// log "RECV_CHAT_MSG TRIGGERED" upon message received, at log level info
        #[arg(long)]
        recv_msg: Option<String>,
    },
}

pub struct SimulationHandler {}

impl SimulationHandler {
    pub async fn run(f: Fledger, command: SimulationCommand) -> anyhow::Result<()> {
        match command.subcommand.clone() {
            SimulationSubcommand::Chat { send_msg, recv_msg } => {
                Self::run_chat(f, command, send_msg, recv_msg).await
            }
        }
    }

    async fn run_chat(
        mut f: Fledger,
        simulation_args: SimulationCommand,
        send_msg: Option<String>,
        recv_msg: Option<String>,
    ) -> anyhow::Result<()> {
        f.loop_node(crate::FledgerState::Connected(1)).await?;

        let node_name = f.args.name.clone().unwrap_or("unknown".into());

        if let Some(ref msg) = recv_msg {
            log::info!("Waiting for chat message {}.", msg);
        }

        if let Some(ref msg) = send_msg {
            log::info!("Sending chat message {}.", msg);
            f.node.add_chat_message(msg.into()).await?;
        }

        let mut acked_msg_ids: Vec<U256> = Vec::new();

        // necessary to grab the variable for lifetime purposes.
        let _influx = Metrics::setup(node_name);

        loop {
            wait_ms(1000).await;

            let fledger_message_total = f.node.gossip.as_ref().unwrap().chat_events().len();
            absolute_counter!(
                "fledger_message_total",
                fledger_message_total.try_into().unwrap()
            );
            increment_counter!("fledger_iterations_total");

            if simulation_args.print_new_messages {
                Self::log_new_messages(&f, &mut acked_msg_ids);
            }

            if let Some(ref msg) = recv_msg {
                let gossip = f.node.gossip.as_ref();
                if gossip
                    .unwrap()
                    .chat_events()
                    .iter()
                    .any(|ev| ev.msg.eq(msg))
                {
                    log::info!("RECV_CHAT_MSG TRIGGERED");
                    f.loop_node(crate::FledgerState::Forever).await?;
                    return Ok(());
                }
            }
        }
    }

    fn log_new_messages(f: &Fledger, acked_msg_ids: &mut Vec<U256>) {
        let chat_events = f.node.gossip.as_ref().unwrap().chat_events();
        let chats: Vec<&Event> = chat_events
            .iter()
            .filter(|ev| !acked_msg_ids.contains(&ev.get_id()))
            .collect();

        if chats.len() <= 0 {
            log::debug!("... No new message");
        } else {
            log::info!("--- New Messages ---");
            for chat in chats {
                acked_msg_ids.push(chat.get_id());
                log::info!("    [{}] {}", chat.src, chat.msg);
            }
        }
    }
}
