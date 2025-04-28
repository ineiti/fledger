use std::any::type_name;

use crate::Fledger;
use clap::{arg, Args, Subcommand};
use flarch::{nodeids::U256, tasks::wait_ms};
use flcrypto::tofrombytes::ToFromBytes;
use flmodules::{
    dht_storage::realm_view::RealmView,
    flo::blob::{BlobAccess, BlobTag},
    gossip_events::core::Event,
};
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

    DhtJoinRealm {},

    CreateTag {
        #[arg(long)]
        tag: String,
    },

    FetchTag {
        #[arg(long)]
        tag: String,
    },
}

pub struct SimulationHandler {}

impl SimulationHandler {
    pub async fn run(f: Fledger, command: SimulationCommand) -> anyhow::Result<()> {
        match command.subcommand.clone() {
            SimulationSubcommand::Chat { send_msg, recv_msg } => {
                Self::run_chat(f, command, send_msg, recv_msg).await
            }
            SimulationSubcommand::DhtJoinRealm {} => Self::run_dht_join_realm(f).await,
            SimulationSubcommand::CreateTag { tag } => Self::run_dht_create_tag(f, tag).await,
            SimulationSubcommand::FetchTag { tag } => Self::run_dht_fetch_tag(f, tag).await,
        }
    }

    async fn run_chat(
        mut f: Fledger,
        simulation_args: SimulationCommand,
        send_msg: Option<String>,
        recv_msg: Option<String>,
    ) -> anyhow::Result<()> {
        f.loop_node(crate::FledgerState::Connected(1)).await?;

        if let Some(ref msg) = recv_msg {
            log::info!("Waiting for chat message {}.", msg);
        }

        if let Some(ref msg) = send_msg {
            log::info!("Sending chat message {}.", msg);
            f.node.add_chat_message(msg.into()).await?;
        }

        let mut acked_msg_ids: Vec<U256> = Vec::new();

        loop {
            wait_ms(1000).await;

            let fledger_message_total = f.node.gossip.as_ref().unwrap().chat_events().len();
            let fledger_connected_total = f.node.nodes_connected()?.len();
            absolute_counter!(
                "fledger_message_total",
                fledger_message_total.try_into().unwrap()
            );
            absolute_counter!("fledger_connected_total", fledger_connected_total as u64);
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
                    log::info!("SIMULATION END");
                    f.loop_node(crate::FledgerState::Forever).await?;
                    return Ok(());
                }
            }
        }
    }

    async fn run_dht_join_realm(mut f: Fledger) -> anyhow::Result<()> {
        f.loop_node(crate::FledgerState::DHTAvailable).await?;
        log::info!("SIMULATION END");

        absolute_counter!("fledger_realms_total", 1);

        f.loop_node(crate::FledgerState::Forever).await?;
        return Ok(());
    }

    async fn run_dht_create_tag(mut f: Fledger, tag: String) -> anyhow::Result<()> {
        f.loop_node(crate::FledgerState::DHTAvailable).await?;
        absolute_counter!("fledger_dht_connected", 1);

        log::info!("DHT CONNECTED");

        //let router = f.node.dht_router.unwrap();
        let ds = f.node.dht_storage.as_mut().unwrap();
        let mut rv = RealmView::new_first(ds.clone()).await?;

        // Send a Flo tag blob
        log::info!("Storing tag in DHT {}.", tag);
        let flo_tag = rv
            .create_tag(&tag, None, flcrypto::access::Condition::Pass, &[])
            .unwrap();
        log::info!(
            "tag {}/{}/{} | {}",
            flo_tag.flo_id(),
            flo_tag.realm_id(),
            flo_tag.version(),
            flo_tag.values().iter().next().unwrap().1,
        );

        let _ = ds.store_flo(flo_tag.flo().clone());
        let _ = ds.propagate();
        ds.broker.settle(Vec::new()).await?;

        log::info!("SIMULATION END");
        absolute_counter!("fledger_simulation_end", 1);

        ds.get_flos(&rv.realm.realm_id())
            .await
            .unwrap()
            .iter()
            .for_each(|flo| {
                let flo_type = type_name::<BlobTag>();
                if flo.flo_type() == flo_type {
                    let tag = BlobTag::from_rmp_bytes(flo_type, &flo.data()).unwrap();
                    log::info!(
                        "tag found {}/{}/{} | {}",
                        flo.flo_id(),
                        flo.realm_id(),
                        flo.version(),
                        tag.0.values().iter().next().unwrap().1,
                    )
                }
            });

        // if let Some(ref tags) = rv.tags {
        //     log::info!("storage amt: {}", tags.storage.iter().count());
        //
        //     let tagname = tags
        //         .storage
        //         .iter()
        //         .next()
        //         .unwrap()
        //         .1 // first tag stored
        //         .values()
        //         .iter()
        //         .next()
        //         .unwrap()
        //         .1; // name of tag
        //     log::info!("tag found: {}", tagname);
        // } else {
        //     log::info!("NOTICE: tag not found.")
        // }

        f.loop_node(crate::FledgerState::Forever).await?;
        return Ok(());
    }

    async fn run_dht_fetch_tag(mut f: Fledger, tag: String) -> anyhow::Result<()> {
        f.loop_node(crate::FledgerState::DHTAvailable).await?;
        absolute_counter!("fledger_dht_connected", 1);

        log::info!("DHT CONNECTED");

        let ds = f.node.dht_storage.as_mut().unwrap();
        let mut rv = RealmView::new_first(ds.clone()).await?;

        loop {
            wait_ms(1000).await;

            // let fledger_connected_total = f.node.nodes_connected()?.len(); // TODO: does not
            // compile.
            //absolute_counter!("fledger_connected_total", fledger_connected_total as u64);
            increment_counter!("fledger_iterations_total");

            rv.update_tags().await?;

            let flos = ds.get_flos(&rv.realm.realm_id()).await.unwrap().clone();
            // flos.iter().for_each(|flo| {
            //     log::info!(
            //         "flo found {}/{}/{} [{}]",
            //         flo.flo_id(),
            //         flo.realm_id(),
            //         flo.version(),
            //         flo.flo_type(),
            //     )
            // });

            let mut tags = flos
                .iter()
                .filter(|flo| flo.flo_type() == type_name::<BlobTag>())
                .map(|flo| BlobTag::from_rmp_bytes(&flo.flo_type(), &flo.data()).unwrap());

            tags.clone().for_each(|tag| {
                log::info!("tag found {}", tag.0.values().iter().next().unwrap().1)
            });

            if tags.any(|flotag| {
                flotag
                    .0
                    .values()
                    .iter()
                    .next()
                    .is_some_and(|tagname| *tagname.1 == tag)
            }) {
                log::info!("SIMULATION END");
                absolute_counter!("fledger_simulation_end", 1);
                f.loop_node(crate::FledgerState::Forever).await?;

                return Ok(());
            } else {
                log::info!("Tag not found...");
            }
            // tags.storage
            //     .iter()
            //     .any(|flotag| flotag.1.values().get(&tag).is_some());
            // //.any(|flotag| flotag.1.values().iter().next().unwrap().1.eq(&tag));
            // //.any(|tag| tag.1.values() tag.1.cache().0.get_blob_mut().values.get(tag).is_some());
            // {
            //     log::info!("SIMULATION END");
            //     absolute_counter!("fledger_simulation_end", 1);
            //     f.loop_node(crate::FledgerState::Forever).await?;
            //
            //     return Ok(());
            // }
        }
    }

    // async fn run_dht_request_random_flow(mut f: Fledger) -> anyhow::Result<()> {
    //     let ds = f.node.dht_storage.unwrap();
    //     let rv = RealmView::new_first(ds.clone()).await?;
    //
    //     // To send requests for random floID
    //     // let realm_id = ds.get_realm_ids().await?.first().unwrap();
    //     let realm_id = rv.realm.realm_id();
    //     let res = ds.get_flo(&GlobalID::new(realm_id, FloID::rnd())).await;
    //
    //     return Ok(());
    // }

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
