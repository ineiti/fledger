use crate::simulation::SimulationCommand;
use crate::Fledger;
use flarch::nodeids::U256;
use flarch::tasks::wait_ms;
use flmodules::gossip_events::core::Event;

#[derive(Clone)]
pub struct SimulationChat {}

impl SimulationChat {
    pub async fn run_chat(
        mut f: Fledger,
        simulation_args: SimulationCommand,
        send_msg: Option<String>,
        recv_msg: Option<String>,
    ) -> anyhow::Result<()> {
        f.loop_node(crate::FledgerState::Connected(1)).await?;

        if let Some(ref msg) = recv_msg {
            log::info!("Waiting for simulation_chat message {}.", msg);
        }

        if let Some(ref msg) = send_msg {
            log::info!("Sending simulation_chat message {}.", msg);
            f.node.add_chat_message(msg.into()).await?;
        }

        let mut acked_msg_ids: Vec<U256> = Vec::new();

        loop {
            wait_ms(1000).await;

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
                    log::info!("SIMULATION END");
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
