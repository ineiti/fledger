#![recursion_limit = "1024"]

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

use console_error_panic_hook::set_once as set_panic_hook;
use ybc::TileCtx::{Ancestor, Child, Parent};
use yew::prelude::*;

use flarch::wait_ms;
use flnet::{
    broker::Broker,
    config::{NodeConfig},
};
use shared::{common::{PPMessage, NodeList}, handler::PingPong};

struct App {
    link: ComponentLink<Self>,
    logs: Vec<String>,
    config: NodeConfig,
}

enum AppMessage {
    Init,
    NewPingPong(Broker<PPMessage>),
    NewMessage(String),
}

impl Component for App {
    type Message = AppMessage;
    type Properties = ();

    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        link.send_message(AppMessage::Init);
        Self {
            link,
            logs: vec![],
            config: flnet::config::NodeConfig::new(),
        }
    }

    fn update(&mut self, msg: Self::Message) -> bool {
        match msg {
            AppMessage::Init => {
                log::info!("Initializing pingpong");
                let link = self.link.clone();
                let config = self.config.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let id = config.info.get_id();
                    let net = flnet::network_start(config, shared::handler::URL)
                        .await
                        .expect("Couldn't start network");

                    let pp = PingPong::new(id, net)
                        .await
                        .expect("Couldn't start pingpong");
                    link.send_message(AppMessage::NewPingPong(pp));
                })
            }
            AppMessage::NewPingPong(mut pp) => {
                log::info!("Starting pingpong");
                let link = self.link.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let mut list = NodeList::new(vec![]);
                    let (tap, _) = pp.get_tap().await.expect("Couldn't get tap");
                    loop {
                        for msg in tap.try_iter() {
                            match msg {
                                PPMessage::FromNetwork(id, msg) => {
                                    link.send_message(AppMessage::NewMessage(format!(
                                        "Received {msg:?} from {}",
                                        list.get_name(&id)
                                    )));
                                }
                                PPMessage::ToNetwork(id, msg) => {
                                    link.send_message(AppMessage::NewMessage(format!(
                                        "Sending {msg:?} to {}",
                                        list.get_name(&id)
                                    )));
                                }
                                PPMessage::List(new_list) => {
                                    if list.update(new_list) {
                                        link.send_message(AppMessage::NewMessage(format!(
                                            "list: {}", list.names()
                                        )));
                                    }
                                }
                                _ => {}
                            }
                        }
                        wait_ms(1000).await;
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

    fn change(&mut self, _: Self::Properties) -> bool {
        false
    }

    fn view(&self) -> Html {
        let logs = self.logs.join("\n");
        html! {
            <>
            <ybc::Navbar
                classes=classes!("is-success")
                padded=true
                navbrand=html!{
                    <ybc::NavbarItem>
                        <ybc::Title classes=classes!("has-text-white") size=ybc::HeaderSize::Is4>{"FLNet Ping-Pong Example"}</ybc::Title>
                    </ybc::NavbarItem>
                }
                navstart=html!{}
                navend=html!{}
            />

            <ybc::Hero
                classes=classes!("is-light")
                size=ybc::HeroSize::FullheightWithNavbar
                body=html!{
                    <ybc::Container classes=classes!("is-centered")>
                    <ybc::Tile ctx=Ancestor>
                        <ybc::Tile ctx=Parent size=ybc::TileSize::Twelve>
                            <ybc::Tile ctx=Parent>
                                <ybc::Tile ctx=Child classes=classes!("content")>
                                <h1>{"FLNet example"}</h1>
                                <h2>{"Ping-pong logs for "}{&self.config.info.name}</h2>
                                <pre>{logs}</pre>
                                </ybc::Tile>
                            </ybc::Tile>
                        </ybc::Tile>
                    </ybc::Tile>
                    </ybc::Container>
                }>
            </ybc::Hero>
            </>
        }
    }

    fn rendered(&mut self, _first_render: bool) {}

    fn destroy(&mut self) {}
}

fn main() {
    set_panic_hook();
    wasm_logger::init(wasm_logger::Config::new(log::Level::Debug));

    yew::start_app::<App>();
}
