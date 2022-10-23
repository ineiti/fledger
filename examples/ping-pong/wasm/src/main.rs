#![recursion_limit = "1024"]

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

use console_error_panic_hook::set_once as set_panic_hook;
use ybc::TileCtx::{Ancestor, Child, Parent};
use yew::prelude::*;

use flarch::now;
use flnet::{broker::Broker, config::NodeConfig};

use shared::{
    common::{NodeList, PPMessage},
    handler::PingPong,
};

struct App {
    link: ComponentLink<Self>,
    logs: Vec<String>,
    config: NodeConfig,
    pp_imp: bool,
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
            pp_imp: now() % 1000 >= 500,
        }
    }

    fn update(&mut self, app_msg: Self::Message) -> bool {
        match app_msg {
            AppMessage::Init => {
                log::info!("Initializing pingpong");
                let link = self.link.clone();
                let config = self.config.clone();
                let pp_imp = self.pp_imp;
                wasm_bindgen_futures::spawn_local(async move {
                    let id = config.info.get_id();
                    let net = flnet::network_broker_start(config, shared::handler::URL)
                        .await
                        .expect("Couldn't start network");

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
                let link = self.link.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let mut list = NodeList::new(vec![]);
                    let (mut tap, _) = pp.get_tap_async().await.expect("Couldn't get tap");
                    loop {
                        match tap.recv().await.expect("Tap has been closed") {
                            PPMessage::FromNetwork(id, ppmsg) => {
                                link.send_message(AppMessage::NewMessage(format!(
                                    "Received {ppmsg:?} from {}",
                                    list.get_name(&id)
                                )));
                            }
                            PPMessage::ToNetwork(id, ppmsg) => {
                                link.send_message(AppMessage::NewMessage(format!(
                                    "Sending {ppmsg:?} to {}",
                                    list.get_name(&id)
                                )));
                            }
                            PPMessage::List(new_list) => {
                                if list.update(new_list) {
                                    link.send_message(AppMessage::NewMessage(format!(
                                        "list: {}",
                                        list.names()
                                    )));
                                }
                            }
                            _ => {}
                        }
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
        let pp_imp = match self.pp_imp{
            true => "Handler",
            false => "Event Loop",
        };
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
                                <h2>{"Running "}{pp_imp}</h2>
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
