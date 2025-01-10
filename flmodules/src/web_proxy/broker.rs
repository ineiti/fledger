use core::str;
use flarch::{
    data_storage::DataStorage,
    platform_async_trait,
    tasks::spawn_local,
    tasks::time::{timeout, Duration},
};
use thiserror::Error;
use tokio::sync::{mpsc::channel, watch};

use crate::{loopix::{PROXY_MESSAGE_BANDWIDTH, PROXY_MESSAGE_COUNT, RETRY_COUNT}, overlay::messages::{NetworkWrapper, OverlayIn, OverlayMessage, OverlayOut}};
use flarch::{
    broker::{Broker, BrokerError, Subsystem, SubsystemHandler},
    nodeids::{NodeID, U256},
};

use super::{
    core::{Counters, WebProxyConfig, WebProxyStorage, WebProxyStorageSave},
    messages::{WebProxyIn, WebProxyMessage, WebProxyMessages, WebProxyOut},
    response::Response,
};

const MODULE_NAME: &str = "WebProxy";

#[derive(Debug, Error)]
pub enum WebProxyError {
    #[error("Didn't get answer from proxy node")]
    TimeoutProxyNode,
    #[error("BrokerError({0})")]
    BrokerError(#[from] BrokerError),
    #[error("No nodes available for proxying")]
    NoNodes,
    #[error("Timeout while waiting for response")]
    ResponseTimeout,
}

#[derive(Clone)]
pub struct WebProxy {
    /// Represents the underlying broker.
    pub web_proxy: Broker<WebProxyMessage>,
    storage: watch::Receiver<WebProxyStorage>,
}

impl WebProxy {
    pub async fn start(
        mut ds: Box<dyn DataStorage + Send>,
        our_id: NodeID,
        overlay: Broker<OverlayMessage>,
        config: WebProxyConfig,
    ) -> Result<Self, WebProxyError> {
        let str = ds.get(MODULE_NAME).unwrap_or("".into());
        let storage = WebProxyStorageSave::from_str(&str).unwrap_or_default();
        let mut web_proxy = Broker::new();
        let messages = WebProxyMessages::new(storage.clone(), config, our_id, web_proxy.clone())?;

        Translate::start(web_proxy.clone(), overlay, messages).await?;

        let (tx, storage) = watch::channel(storage);
        let (mut tap, _) = web_proxy.get_tap().await?;
        spawn_local(async move {
            loop {
                if let Some(WebProxyMessage::Output(WebProxyOut::UpdateStorage(sto))) =
                    tap.recv().await
                {
                    tx.send(sto.clone()).expect("updated storage");
                    if let Ok(val) = sto.to_yaml() {
                        ds.set(MODULE_NAME, &val).expect("updating storage");
                    }
                }
            }
        });

        Ok(Self { web_proxy, storage })
    }

    /// Sends a GET request to one of the remote proxies with the given URL.
    /// If the remote proxy doesn't answer within 5 seconds, a timeout error is
    /// returned.
    /// TODO: add GET headers and body, move timeout to configuration
    pub async fn get(&mut self, url: &str) -> Result<Response, WebProxyError> {
        log::debug!("Getting {url}");
        let our_rnd = U256::rnd();
        let (tx, rx) = channel(128);
        self.web_proxy
            .emit_msg(WebProxyIn::RequestGet(our_rnd, url.to_string(), tx).into())?;
        let (mut tap, id) = self.web_proxy.get_tap().await?;
        timeout(Duration::from_secs(5), async move {
            while let Some(msg) = tap.recv().await {
                if let WebProxyMessage::Output(WebProxyOut::ResponseGet(proxy, rnd, header)) = msg {
                    if rnd == our_rnd {
                        self.web_proxy.remove_subsystem(id).await?;
                        return Ok(Response::new(proxy, header, rx));
                    }
                }
            }
            self.web_proxy.remove_subsystem(id).await?;
            return Err(WebProxyError::ResponseTimeout);
        })
        .await
        .map_err(|_| WebProxyError::ResponseTimeout)?
    }

    pub async fn get_with_retry_and_timeout(&mut self, url: &str, retry: u8, time: Duration) -> Result<Response, WebProxyError> {
        for i in 0..(retry + 1) { // +1 because it should retry at least once
            if i != 0 {
                RETRY_COUNT.inc();
            }
            log::debug!("Getting {url} with retry {i}");
            match self.get_with_timeout(url, time).await {
                Ok(resp) => return Ok(resp),
                Err(e) => log::error!("Error getting {url}: {e}"),
            }
        }
        Err(WebProxyError::ResponseTimeout)
    }

    pub async fn get_with_timeout(&mut self, url: &str, time: Duration) -> Result<Response, WebProxyError> {
        log::debug!("Getting {url}");
        let our_rnd = U256::rnd();
        let (tx, rx) = channel(128);
        self.web_proxy
            .emit_msg(WebProxyIn::RequestGet(our_rnd, url.to_string(), tx).into())?;
        let (mut tap, id) = self.web_proxy.get_tap().await?;
        timeout(time, async move {
            while let Some(msg) = tap.recv().await {
                if let WebProxyMessage::Output(WebProxyOut::ResponseGet(proxy, rnd, header)) = msg {
                    if rnd == our_rnd {
                        self.web_proxy.remove_subsystem(id).await?;
                        return Ok(Response::new(proxy, header, rx));
                    }
                }
            }
            self.web_proxy.remove_subsystem(id).await?;
            return Err(WebProxyError::ResponseTimeout);
        })
        .await
        .map_err(|_| WebProxyError::ResponseTimeout)?
    }

    pub fn get_counters(&mut self) -> Counters {
        self.storage.borrow().counters.clone()
    }
}

struct Translate {
    messages: WebProxyMessages,
}

impl Translate {
    async fn start(
        mut web_proxy: Broker<WebProxyMessage>,
        overlay: Broker<OverlayMessage>,
        messages: WebProxyMessages,
    ) -> Result<(), WebProxyError> {
        web_proxy
            .add_subsystem(Subsystem::Handler(Box::new(Translate { messages })))
            .await?;
        web_proxy
            .link_bi(
                overlay,
                Box::new(Self::link_overlay_proxy),
                Box::new(Self::link_proxy_overlay),
            )
            .await?;
        Ok(())
    }

    fn link_overlay_proxy(msg: OverlayMessage) -> Option<WebProxyMessage> {
        if let OverlayMessage::Output(msg_out) = msg {
            match msg_out {
                OverlayOut::NodeInfosConnected(list) => {
                    Some(WebProxyIn::NodeInfoConnected(list).into())
                }
                OverlayOut::NetworkWrapperFromNetwork(id, msg) => msg
                    .unwrap_yaml(MODULE_NAME)
                    .map(|msg| WebProxyIn::FromNetwork(id, msg).into()),
                _ => None,
            }
        } else {
            None
        }
    }

    fn link_proxy_overlay(msg: WebProxyMessage) -> Option<OverlayMessage> {
        if let WebProxyMessage::Output(WebProxyOut::ToNetwork(id, msg_node)) = msg {
            let wrapper = NetworkWrapper::wrap_yaml(MODULE_NAME, &msg_node).unwrap();
            PROXY_MESSAGE_BANDWIDTH.inc_by(wrapper.msg.len() as f64);
            PROXY_MESSAGE_COUNT.inc();
            Some(
                OverlayIn::NetworkWrapperToNetwork(
                    id,
                    wrapper,
                )
                .into(),
            )
        } else {
            None
        }
    }
}

#[platform_async_trait()]
impl SubsystemHandler<WebProxyMessage> for Translate {
    async fn messages(&mut self, msgs: Vec<WebProxyMessage>) -> Vec<WebProxyMessage> {
        let msgs_in = msgs
            .into_iter()
            .filter_map(|msg| match msg {
                WebProxyMessage::Input(msg_in) => Some(msg_in),
                _ => None,
            })
            .collect();
        self.messages
            .process_messages(msgs_in)
            .into_iter()
            .map(|o| o.into())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use flarch::{data_storage::DataStorageTemp, start_logging_filter_level, tasks::wait_ms};

    use crate::nodeconfig::NodeConfig;

    use super::*;

    #[tokio::test]
    async fn test_get() -> Result<(), WebProxyError> {
        start_logging_filter_level(vec![], log::LevelFilter::Debug);
        let cl_ds = Box::new(DataStorageTemp::new());
        let cl_in = NodeConfig::new().info;
        let cl_id = cl_in.get_id();
        let mut cl_rnd = Broker::new();

        let wp_ds = Box::new(DataStorageTemp::new());
        let wp_in = NodeConfig::new().info;
        let wp_id = wp_in.get_id();
        let mut wp_rnd = Broker::new();

        let mut cl =
            WebProxy::start(cl_ds, cl_id, cl_rnd.clone(), WebProxyConfig::default()).await?;
        let (mut cl_tap, _) = cl_rnd.get_tap().await?;
        let _wp = WebProxy::start(wp_ds, wp_id, wp_rnd.clone(), WebProxyConfig::default()).await?;
        let (mut wp_tap, _) = wp_rnd.get_tap().await?;

        let list = vec![cl_in, wp_in];
        cl_rnd.emit_msg(OverlayMessage::Output(OverlayOut::NodeInfosConnected(
            list.clone(),
        )))?;
        wp_rnd.emit_msg(OverlayMessage::Output(OverlayOut::NodeInfosConnected(list)))?;

        let (tx, mut rx) = channel(1);
        spawn_local(async move {
            let mut resp = cl.get("https://fledg.re").await.expect("fetching fledg.re");
            log::debug!("Got response struct with headers: {resp:?}");
            let content = resp.text().await.expect("getting text");
            log::debug!("Got text from content: {content:?}");
            tx.send(1).await.expect("sending done");
        });

        loop {
            if let Ok(_) = rx.try_recv() {
                log::debug!("Done");
                return Ok(());
            }

            if let Ok(OverlayMessage::Input(OverlayIn::NetworkWrapperToNetwork(dst, msg))) =
                cl_tap.try_recv()
            {
                log::debug!("Sending to WP: {msg:?}");
                wp_rnd
                    .emit_msg(OverlayMessage::Output(OverlayOut::NetworkWrapperFromNetwork(
                        dst, msg,
                    )))
                    .expect("sending to wp");
            }

            if let Ok(OverlayMessage::Input(OverlayIn::NetworkWrapperToNetwork(dst, msg))) =
                wp_tap.try_recv()
            {
                log::debug!("Sending to CL: {msg:?}");
                cl_rnd
                    .emit_msg(OverlayMessage::Output(OverlayOut::NetworkWrapperFromNetwork(
                        dst, msg,
                    )))
                    .expect("sending to wp");
            }

            log::debug!("Waiting");
            wait_ms(100).await;
        }
    }
}