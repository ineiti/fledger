#![recursion_limit = "1024"]

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

use console_error_panic_hook::set_once as set_panic_hook;
use flmodules::{nodeconfig::NodeConfig, timer::Timer};
use tokio::select;
use ybc::TileCtx::{Ancestor, Child, Parent};
use yew::prelude::*;

use flarch::{broker::Broker, tasks::now, web_rtc::connection::ConnectionConfig};

use shared::{
    common::{NodeList, PingPongIn, PingPongOut},
    handler::PingPong,
};

struct App {
    logs: Vec<String>,
    config: NodeConfig,
    pp_imp: bool,
}

enum AppMessage {
    Init,
    NewPingPong(Broker<PingPongIn, PingPongOut>),
    NewMessage(String),
}

impl Component for App {
    type Message = AppMessage;
    type Properties = ();

    fn create(s: &yew::Context<App>) -> Self {
        s.link().send_message(AppMessage::Init);
        Self {
            logs: vec![],
            config: NodeConfig::new(),
            pp_imp: now() % 1000 >= 500,
        }
    }

    fn update(&mut self, s: &yew::Context<App>, app_msg: Self::Message) -> bool {
        match app_msg {
            AppMessage::Init => {
                log::info!("Initializing pingpong");
                let link = s.link().clone();
                let config = self.config.clone();
                let pp_imp = self.pp_imp;
                wasm_bindgen_futures::spawn_local(async move {
                    let id = config.info.get_id();
                    let net = flmodules::network::network_start(
                        config,
                        ConnectionConfig::from_signal(shared::handler::URL),
                        &mut Timer::start().await.expect("Start timer"),
                    )
                    .await
                    .expect("Couldn't start network")
                    .broker;

                    let pp = match pp_imp {
                        true => PingPong::new(id, net)
                            .await
                            .expect("Couldn't start handler pingpong"),
                        false => shared::event_loop::start(id, net)
                            .await
                            .expect("Couldn't start event_loop PingPong"),
                    };

                    link.send_message(AppMessage::NewPingPong(pp));
                })
            }
            AppMessage::NewPingPong(mut pp) => {
                log::info!("Starting pingpong");
                let link = s.link().clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let mut list = NodeList::new(vec![]);
                    let mut tap_in = pp.get_tap_in().await.expect("Couldn't get tap").0;
                    let mut tap_out = pp.get_tap_out().await.expect("Couldn't get tap").0;
                    select! {
                        Some(msg) = tap_in.recv() => {
                            match msg{
                                PingPongIn::FromNetwork(from, ppmsg) => {
                                    link.send_message(AppMessage::NewMessage(format!(
                                        "Received {ppmsg:?} from {}",
                                        list.get_name(&from)
                                    )));
                                }
                                PingPongIn::List(new_list) => {
                                    if list.update(new_list) {
                                        link.send_message(AppMessage::NewMessage(format!(
                                            "list: {}",
                                            list.names()
                                        )));
                                    }
                                }
                                _ => {}
                            }
                        },
                        Some(msg) = tap_out.recv() => {
                            match msg {
                                PingPongOut::ToNetwork(to, ppmsg) => {
                                    link.send_message(AppMessage::NewMessage(format!(
                                        "Sending {ppmsg:?} to {}",
                                        list.get_name(&to)
                                    )));
                                }
                                _ => {}
                            }
                        },
                    }
                })
            }
            AppMessage::NewMessage(s) => {
                self.logs.insert(
                    0,
                    format!("{:?}: {}", js_sys::Date::new_0().to_string().to_string(), s),
                );
                return true;
            }
        }
        false
    }

    fn changed(&mut self, _s: &yew::Context<App>, _: &Self::Properties) -> bool {
        false
    }

    fn view(&self, _s: &yew::Context<App>) -> Html {
        let logs = self.logs.join("\n");
        let pp_imp = match self.pp_imp {
            true => "Handler",
            false => "Event Loop",
        };
        html! {
            <>
            <ybc::Navbar
                classes={classes!("is-success")}
                padded=true
                navbrand={html!{
                    <ybc::NavbarItem>
                        <ybc::Title
                            classes={classes!("has-text-white")}
                            size={ybc::HeaderSize::Is4}>
                            {"flmodules::network Ping-Pong Example"}
                        </ybc::Title>
                    </ybc::NavbarItem>
                }}
                navstart={html!{}}
                navend={html!{}}
            />

            <ybc::Hero
                classes={classes!("is-light")}
                size={ybc::HeroSize::FullheightWithNavbar}
                body={html!{
                    <ybc::Container classes={classes!("is-centered")}>
                    <ybc::Tile ctx={Ancestor}>
                        <ybc::Tile ctx={Parent} size={ybc::TileSize::Twelve}>
                            <ybc::Tile ctx={Parent}>
                                <ybc::Tile ctx={Child} classes={classes!("content")}>
                                <h1>{"flmodules::network example"}</h1>
                                <h2>{"Running "}{pp_imp}</h2>
                                <h2>{"Ping-pong logs for "}{&self.config.info.name}</h2>
                                <pre>{logs}</pre>
                                </ybc::Tile>
                            </ybc::Tile>
                        </ybc::Tile>
                    </ybc::Tile>
                    </ybc::Container>
                }}>
            </ybc::Hero>
            </>
        }
    }

    fn rendered(&mut self, _s: &yew::Context<App>, _first_render: bool) {}

    fn destroy(&mut self, _s: &yew::Context<App>) {}
}

fn main() {
    set_panic_hook();
    wasm_logger::init(wasm_logger::Config::new(log::Level::Debug));

    yew::Renderer::<App>::new().render();
}
