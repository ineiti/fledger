use flarch::tasks::spawn_local_nosend;
use flmodules::web_proxy::broker::WebProxy;
use flnode::stat::NetStats;
use regex::Regex;
use tokio::sync::{broadcast, watch};

use crate::web::{Button, Web};

pub struct Proxy {
    proxy: WebProxy,
    web: Web,
    stats: watch::Receiver<NetStats>,
}

impl Proxy {
    pub fn new(
        proxy: WebProxy,
        stats: watch::Receiver<NetStats>,
        mut rx: broadcast::Receiver<Button>,
    ) -> anyhow::Result<()> {
        let mut p = Proxy {
            proxy,
            web: Web::new()?,
            stats,
        };

        spawn_local_nosend(async move {
            loop {
                tokio::select! {
                    Ok(btn) = rx.recv() => p.clicked(btn).await,
                }
            }
        });
        Ok(())
    }

    async fn clicked(&mut self, btn: Button) {
        match btn {
            Button::ProxyRequest => {
                let proxy_url = self.web.get_input("proxy_url");
                if proxy_url.value() != "" {
                    let proxy_button = self.web.get_button("proxy_request");
                    proxy_button.set_disabled(true);
                    let mut url = proxy_url.value();
                    if !Regex::new("^https?://").unwrap().is_match(&url) {
                        url = format!("https://{url}");
                    }
                    let proxy_div = self.web.get_div("proxy_div");
                    let mut webproxy = self.proxy.clone();
                    let nodes = self.stats.borrow().node_infos_connected();
                    spawn_local_nosend(async move {
                        let fetching = format!("Fetching url from proxy: {}", url);
                        proxy_div.set_inner_html(&fetching);
                        match webproxy.get(&url).await {
                            Ok(mut response) => {
                                let mut proxy_str = format!("{}", response.proxy());
                                if let Some(info) =
                                    nodes.iter().find(|&node| node.get_id() == response.proxy())
                                {
                                    proxy_str = format!("{} ({})", info.name, info.get_id());
                                }
                                let text = format!(
                                    "Proxy: {proxy_str}<br>{}",
                                    response.text().await.unwrap()
                                );
                                proxy_div.set_inner_html(&text);
                            }
                            Err(e) => {
                                let text = format!("Got error while fetching page from proxy: {e}");
                                proxy_div.set_inner_html(&text);
                            }
                        }
                        proxy_button.set_disabled(false);
                    });
                }
            }
            _ => {}
        }
    }
}
