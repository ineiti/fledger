use std::{any::type_name, time::Duration};

use crate::Fledger;
use anyhow::Error;
use clap::{arg, Args, Subcommand};
use flarch::{nodeids::U256, tasks::wait_ms};
use flcrypto::tofrombytes::ToFromBytes;
use flmodules::{
    dht_storage::realm_view::RealmView,
    flo::{
        blob::{BlobAccess, BlobPage, BlobTag},
        flo::FloWrapper,
        realm::GlobalID,
    },
    gossip_events::core::Event,
};
use metrics::{absolute_counter, increment_counter};
use tokio::time::{timeout, Instant};

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

    CreatePageWithFillers {
        #[arg(long)]
        filler_amount: u32,

        #[arg(long)]
        page_size: u32,

        #[arg(long)]
        settling_delay: u32,
    },
    FetchPage {
        #[arg(long, default_value = "20000")]
        timeout_ms: u32,
    },
    WaitPages {
        #[arg(long, default_value = "20000")]
        timeout_ms: u32,

        #[arg(long, default_value = "10")]
        amount: u32,
    },
}

pub struct SimulationHandler {}

impl SimulationHandler {
    pub async fn run(
        f: Fledger,
        command: SimulationCommand,
        sampling_rate_ms: u32,
    ) -> anyhow::Result<()> {
        match command.subcommand.clone() {
            SimulationSubcommand::Chat { send_msg, recv_msg } => {
                Self::run_chat(f, command, send_msg, recv_msg).await
            }
            SimulationSubcommand::DhtJoinRealm {} => Self::run_dht_join_realm(f).await,
            SimulationSubcommand::CreateTag { tag } => Self::run_dht_create_tag(f, tag).await,
            SimulationSubcommand::FetchTag { tag } => Self::run_dht_fetch_tag(f, tag).await,
            SimulationSubcommand::CreatePageWithFillers {
                filler_amount,
                page_size,
                settling_delay,
            } => {
                Self::run_dht_create_page_with_fillers(f, filler_amount, page_size, settling_delay)
                    .await
            }
            SimulationSubcommand::FetchPage { timeout_ms } => {
                Self::run_dht_fetch_simulation_page(f, sampling_rate_ms, timeout_ms).await
            }
            SimulationSubcommand::WaitPages { timeout_ms, amount } => {
                Self::run_dht_wait_for_pages(f, sampling_rate_ms, timeout_ms, amount).await
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

        ds.get_flos().await.unwrap().iter().for_each(|flo| {
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

        let tags = rv.tags;

        log::info!("storage amt: {}", tags.storage.iter().count());

        let tagname = tags
            .storage
            .iter()
            .next()
            .unwrap()
            .1 // first tag stored
            .values()
            .iter()
            .next()
            .unwrap()
            .1; // name of tag
        log::info!("tag found: {}", tagname);

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

            let flos = ds.get_flos().await.unwrap().clone();
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

    async fn run_dht_create_page_with_fillers(
        mut f: Fledger,
        filler_amount: u32,
        page_size: u32,
        settling_delay: u32,
    ) -> anyhow::Result<()> {
        f.loop_node(crate::FledgerState::DHTAvailable).await?;
        absolute_counter!("fledger_dht_connected", 1);

        log::info!("DHT CONNECTED");

        //let router = f.node.dht_router.unwrap();
        let ds = f.node.dht_storage.as_mut().unwrap();
        let mut rv = RealmView::new_first(ds.clone()).await?;

        log::info!("[Create filler pages]");
        for i in 0..filler_amount {
            let flo_page = rv
                .create_http(
                    &format!("simulation-filler-{}", i.to_string()),
                    String::from_utf8(vec![b'-'; page_size as usize])?,
                    None,
                    flcrypto::access::Condition::Pass,
                    &[],
                )
                .await
                .unwrap();

            let page_content =
                String::from_utf8(flo_page.datas().iter().next().unwrap().1.clone().to_vec())
                    .unwrap();

            log::info!(
                "page {}/{}/{} | {} | {} ({}B -> {}B)",
                flo_page.flo_id(),
                flo_page.realm_id(),
                flo_page.version(),
                flo_page.values().iter().next().unwrap().1,
                page_content.chars().take(50).collect::<String>(),
                page_content.size(),
                flo_page.size(),
            );
        }

        log::info!("[Waiting for fillers to settle]");
        log::info!("{} ms", settling_delay);

        let settling_seconds = settling_delay / 1000;
        for _ in 0..settling_seconds {
            ds.propagate()?;
            ds.sync()?;
            ds.broker.settle(Vec::new()).await?;
            wait_ms(1000).await;
        }

        wait_ms((settling_delay % 1000) as u64).await;

        log::info!("[Sending simulation flo page]");
        let flo_page = rv
            .create_http(
                "simulation-page",
                String::from_utf8(vec![b'o'; page_size as usize])?,
                None,
                flcrypto::access::Condition::Pass,
                &[],
            )
            .await
            .unwrap();

        let page_content =
            String::from_utf8(flo_page.datas().iter().next().unwrap().1.clone().to_vec()).unwrap();

        log::info!(
            "page {}/{}/{} | {} | {} ({}B -> {}B)",
            flo_page.flo_id(),
            flo_page.realm_id(),
            flo_page.version(),
            flo_page.values().iter().next().unwrap().1,
            page_content.chars().take(50).collect::<String>(),
            page_content.size(),
            flo_page.size(),
        );

        let signer = f.node.crypto_storage.get_signer();
        rv.set_realm_service("simulation-page", flo_page.blob_id(), &[&signer])
            .await?;

        ds.store_flo(flo_page.flo().clone())?;
        ds.propagate()?;
        ds.store_flo(flo_page.flo().clone())?;
        ds.propagate()?;
        ds.store_flo(flo_page.flo().clone())?;
        ds.propagate()?;
        ds.store_flo(flo_page.flo().clone())?;
        ds.propagate()?;

        ds.sync()?;
        ds.sync()?;

        ds.broker.settle(Vec::new()).await?;

        log::info!("SIMULATION END");
        absolute_counter!("fledger_simulation_success", 1);

        f.loop_node(crate::FledgerState::Forever).await?;
        return Ok(());
    }

    async fn run_dht_wait_for_pages(
        mut f: Fledger,
        sampling_rate_ms: u32,
        timeout_ms: u32,
        amount: u32,
    ) -> anyhow::Result<()> {
        let start_instant = Instant::now();
        absolute_counter!("fledger_simulation_success", 0);
        absolute_counter!("fledger_dht_connected", 0);
        absolute_counter!("fledger_connected_total", 0);

        let timeout_result = timeout(
            Duration::from_millis(timeout_ms.into()),
            f.loop_node(crate::FledgerState::DHTAvailable),
        )
        .await;

        if timeout_result.is_err() {
            log::warn!("SIMULATION TIMEOUT WHILE CONNECTING TO DHT");
            log::info!("SIMULATION END");
            absolute_counter!("fledger_simulation_timeout", 1);
        }

        absolute_counter!("fledger_dht_connected", 1);

        log::info!("DHT CONNECTED");

        let ds = f.node.dht_storage.as_mut().unwrap();
        let mut rv = RealmView::new_first(ds.clone()).await?;

        loop {
            if start_instant.elapsed().as_millis() > timeout_ms as u128 {
                log::warn!("SIMULATION TIMEOUT REACHED ({}ms)", timeout_ms);
                log::info!("SIMULATION END");
                absolute_counter!("fledger_simulation_timeout", 1);
                f.loop_node(crate::FledgerState::Forever).await?;

                return Ok(());
            }

            wait_ms(sampling_rate_ms.into()).await;

            increment_counter!("fledger_iterations_total");

            absolute_counter!(
                "fledger_connected_total",
                f.node.nodes_connected()?.len() as u64
            );

            rv.update_all().await?;
            f.node.dht_storage.as_mut().unwrap().sync()?;
            f.node.dht_storage.as_mut().unwrap().propagate()?;

            let pages = f
                .node
                .dht_storage
                .as_mut()
                .unwrap()
                .get_flos()
                .await
                .unwrap()
                .clone();
            let pages = pages
                .iter()
                .filter(|flo| flo.flo_type() == type_name::<BlobPage>())
                .map(|flo| BlobPage::from_rmp_bytes(&flo.flo_type(), &flo.data()).unwrap());

            let page_count = pages.count();
            absolute_counter!("fledger_pages_total", page_count as u64);

            if page_count >= amount as usize {
                log::info!("enough pages received");
                log::info!("SIMULATION END");
                f.loop_node(crate::FledgerState::Forever).await?;
                return Ok(());
            } else {
                log::info!("NOT enough pages received...");
            }
        }
    }

    async fn run_dht_fetch_simulation_page(
        mut f: Fledger,
        sampling_rate_ms: u32,
        timeout_ms: u32,
    ) -> anyhow::Result<()> {
        let start_instant = Instant::now();
        absolute_counter!("fledger_simulation_success", 0);

        let timeout_result = timeout(
            Duration::from_millis(timeout_ms.into()),
            f.loop_node(crate::FledgerState::DHTAvailable),
        )
        .await;

        if timeout_result.is_err() {
            log::warn!("SIMULATION TIMEOUT WHILE CONNECTING TO DHT");
            log::info!("SIMULATION END");
            absolute_counter!("fledger_simulation_timeout", 1);
        }

        absolute_counter!("fledger_dht_connected", 1);

        log::info!("DHT CONNECTED");

        let ds = f.node.dht_storage.as_mut().unwrap();
        let mut rv = RealmView::new_first(ds.clone()).await?;

        loop {
            if start_instant.elapsed().as_millis() > timeout_ms as u128 {
                log::warn!("SIMULATION TIMEOUT REACHED ({}ms)", timeout_ms);
                log::info!("SIMULATION END");
                absolute_counter!("fledger_simulation_timeout", 1);
                f.loop_node(crate::FledgerState::Forever).await?;

                return Ok(());
            }

            wait_ms(sampling_rate_ms.into()).await;

            increment_counter!("fledger_iterations_total");

            rv.update_all().await?;

            let pages = ds.get_flos().await.unwrap().clone();
            let pages = pages
                .iter()
                .filter(|flo| flo.flo_type() == type_name::<BlobPage>())
                .map(|flo| BlobPage::from_rmp_bytes(&flo.flo_type(), &flo.data()).unwrap());

            let mut simulation_page_stored_this_iteration = false;
            pages.clone().for_each(|page| {
                let page_name = page.0.values().iter().next().unwrap().1;
                log::info!("page found {}", page_name);
                if page_name == "simulation-page" {
                    simulation_page_stored_this_iteration = true;
                }
            });

            if simulation_page_stored_this_iteration {
                absolute_counter!("fledger_simulation_page_stored", 1);
            } else {
                absolute_counter!("fledger_simulation_page_stored", 0);
            }

            absolute_counter!("fledger_pages_total", pages.count() as u64);

            let ds_size = ds.stats.borrow().realm_stats.iter().next().unwrap().1.size;
            absolute_counter!("fledger_realm_storage_bytes", ds_size as u64);

            let page_id_opt = rv.realm.cache().get_services().get("simulation-page");
            if let Some(page_id) = page_id_opt {
                let page_global_id = GlobalID::new(rv.realm.realm_id(), page_id.clone());
                let page_flo_wrapper_result: Result<FloWrapper<BlobPage>, Error> =
                    ds.get_flo(&page_global_id).await;

                if let Ok(page_flo_wrapper) = page_flo_wrapper_result {
                    let page_flo = page_flo_wrapper.flo();
                    let page_blob =
                        BlobPage::from_rmp_bytes(page_flo.flo_type().as_str(), page_flo.data())
                            .unwrap();
                    let page_content = String::from_utf8(
                        page_blob
                            .0
                            .datas()
                            .iter()
                            .next()
                            .unwrap()
                            .1
                            .clone()
                            .to_vec(),
                    )
                    .unwrap_or_default();
                    log::info!(
                        "simulation page found with content: {}",
                        page_content.chars().take(50).collect::<String>()
                    );

                    log::info!("SIMULATION END");
                    absolute_counter!("fledger_simulation_success", 1);
                    f.loop_node(crate::FledgerState::Forever).await?;

                    return Ok(());
                } else {
                    increment_counter!("fledger_simulation_page_fetch_fail_total");

                    log::info!("could not fetch page with id [{}].", page_id);
                }
            } else {
                increment_counter!("fledger_simulation_service_fetch_fail_total");

                log::info!("page_id not found in services...");
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
