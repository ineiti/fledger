use log::{error, info};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{mpsc::channel, Arc, Mutex},
};

use common::{
    node::Node,
    signal::{
        web_rtc::{
            WSSignalMessage, WebRTCConnection, WebRTCConnectionSetup, WebRTCConnectionState,
            WebRTCSetupCBMessage, WebSocketMessage,
        },
        websocket::{MessageCallback, WSMessage, WebSocketConnection},
    },
    types::DataStorage,
    types::U256,
};

use wasm_bindgen_test::*;

use crate::helpers::wait_ms;
use crate::web_rtc_setup::WebRTCConnectionSetupWasm;

#[derive(Debug)]
struct Message {
    str: String,
    id: u32,
}

struct WebSocketDummy {
    msg_queue: Rc<RefCell<Vec<Message>>>,
    // connections: Vec<WebSocketConnectionDummy>,
    callbacks: Vec<Rc<RefCell<Option<MessageCallback>>>>,
}

impl WebSocketDummy {
    fn new() -> WebSocketDummy {
        WebSocketDummy {
            msg_queue: Rc::new(RefCell::new(vec![])),
            // connections: vec![],
            callbacks: vec![],
        }
    }

    fn get_connection(&mut self) -> Result<WebSocketConnectionDummy, String> {
        let id = self.callbacks.len() as u32;
        if id > 1 {
            return Err("currently only supports 2 nodes".to_string());
        }
        let wscd = WebSocketConnectionDummy::new(Rc::clone(&self.msg_queue), id);
        self.callbacks.push(Rc::clone(&wscd.cb));
        let wsm_str = WebSocketMessage {
            msg: WSSignalMessage::Challenge(1u64, U256::rnd()),
        }
        .to_string();
        self.msg_queue
            .borrow_mut()
            .push(Message { id, str: wsm_str });
        Ok(wscd)
    }

    fn run_queue(&mut self) -> Result<usize, String> {
        let msgs: Vec<Message> = self.msg_queue.borrow_mut().drain(..).collect();
        let msgs_count = msgs.len();
        msgs.iter()
            .for_each(|msg| match WebSocketMessage::from_str(&msg.str) {
                Ok(wsm) => match wsm.msg {
                    WSSignalMessage::PeerSetup(_) => {
                        if self.callbacks.len() == 2 {
                            if let Err(e) = self.send_message(1 - msg.id as usize, msg.str.clone())
                            {
                                error!("couldn't push message: {}", e);
                            }
                        }
                    }
                    _ => {}
                },
                Err(e) => info!("Error while getting message: {}", e),
            });
        Ok(msgs_count)
    }

    fn send_message(&mut self, id: usize, msg: String) -> Result<(), String> {
        if self.callbacks.len() <= id {
            return Err("no callback defined yet".to_string());
        }
        if let Some(cb_ref) = self.callbacks.get_mut(id) {
            if let Some(cb) = cb_ref.borrow_mut().as_mut() {
                cb(WSMessage::MessageString(msg));
                return Ok(());
            }
        }
        Err("no callback defined yet".to_string())
    }
}

struct WebSocketConnectionDummy {
    msg_queue: Rc<RefCell<Vec<Message>>>,
    id: u32,
    cb: Rc<RefCell<Option<MessageCallback>>>,
}

impl WebSocketConnectionDummy {
    fn new(msg_queue: Rc<RefCell<Vec<Message>>>, id: u32) -> WebSocketConnectionDummy {
        WebSocketConnectionDummy {
            msg_queue,
            id,
            cb: Rc::new(RefCell::new(None)),
        }
    }
}

impl WebSocketConnection for WebSocketConnectionDummy {
    fn set_cb_wsmessage(&mut self, cb: MessageCallback) {
        self.cb.borrow_mut().replace(cb);
    }

    fn send(&mut self, msg: String) -> Result<(), String> {
        let queue = Rc::clone(&self.msg_queue);
        queue.borrow_mut().push(Message {
            id: self.id,
            str: msg.clone(),
        });
        Ok(())
    }

    fn reconnect(&mut self) -> Result<(), String> {
        todo!()
    }
}

pub struct DataStorageDummy {}

impl DataStorage for DataStorageDummy {
    fn load(&self, _key: &str) -> Result<String, String> {
        Ok("".to_string())
    }

    fn save(&self, _key: &str, _value: &str) -> Result<(), String> {
        Ok(())
    }
}

fn put_msg(
    ice: &Arc<Mutex<Vec<String>>>,
    conn: &Arc<Mutex<Option<Box<dyn WebRTCConnection>>>>,
    msg: WebRTCSetupCBMessage,
) {
    match msg {
        WebRTCSetupCBMessage::Ice(s) => ice.lock().unwrap().push(s),
        WebRTCSetupCBMessage::Connection(c) => {
            conn.lock().unwrap().replace(c);
        }
    }
}

async fn set_callback(
    webrtc: &mut Box<dyn WebRTCConnectionSetup>,
    ice: &Arc<Mutex<Vec<String>>>,
    conn: &Arc<Mutex<Option<Box<dyn WebRTCConnection>>>>,
) {
    let icec = Arc::clone(&ice);
    let connc = Arc::clone(&conn);
    webrtc
        .set_callback(Box::new(move |msg| {
            info!("dbg: Got message: {:?}", &msg);
            put_msg(&icec, &connc, msg);
        }))
        .await;
}

async fn connect_test_base() -> Result<(), String> {
    info!("Setting up nodes");
    // First node
    let ice1 = Arc::new(Mutex::new(vec![]));
    let conn1 = Arc::new(Mutex::new(None));
    let mut webrtc1 = WebRTCConnectionSetupWasm::new(WebRTCConnectionState::Initializer)?;
    set_callback(&mut webrtc1, &ice1, &conn1).await;

    // Second node
    let ice2 = Arc::new(Mutex::new(vec![]));
    let conn2 = Arc::new(Mutex::new(None));
    let mut webrtc2 = WebRTCConnectionSetupWasm::new(WebRTCConnectionState::Follower)?;
    set_callback(&mut webrtc2, &ice2, &conn2).await;

    // Exchange messages
    let offer = webrtc1.make_offer().await?;
    let answer = webrtc2.make_answer(offer).await?;
    webrtc1.use_answer(answer).await?;

    info!("Waiting 2 seconds");
    wait_ms(2000).await;

    // Send ice strings to the other node
    let msgs1: Vec<String> = { ice1.lock().unwrap().splice(.., vec![]).collect() };
    for msg in msgs1 {
        info!("Got message from 1 {:?}", msg);
        webrtc2.ice_put(msg).await?;
    }

    // Send ice strings to the other node
    let msgs2: Vec<String> = { ice2.lock().unwrap().splice(.., vec![]).collect() };
    for msg in msgs2 {
        info!("Got message from 2 {:?}", msg);
        webrtc1.ice_put(msg).await?;
    }

    info!("Waiting 2 seconds");
    wait_ms(2000).await;

    info!("Sending messages");
    // Make sure the connection arrived at the other end
    let conn1l = conn1.lock().unwrap().take().unwrap();
    let conn2l = conn2.lock().unwrap().take().unwrap();

    let msgs1 = Arc::new(Mutex::new(vec![]));
    let msgs2 = Arc::new(Mutex::new(vec![]));
    let msgs1cl = Arc::clone(&msgs1);
    let msgs2cl = Arc::clone(&msgs2);
    conn1l.set_cb_message(Box::new(move |msg| msgs1cl.lock().unwrap().push(msg)));
    conn2l.set_cb_message(Box::new(move |msg| msgs2cl.lock().unwrap().push(msg)));

    conn1l.send("msg1".to_string())?;
    conn2l.send("msg2".to_string())?;

    info!("Waiting 2 seconds");
    wait_ms(2000).await;

    {
        info!(
            "Messages are: {:?} - {:?}",
            msgs1.lock().unwrap().get(0),
            msgs2.lock().unwrap().get(0)
        );
    }

    assert_eq!(&"msg2".to_string(), msgs1.lock().unwrap().get(0).unwrap());
    assert_eq!(&"msg1".to_string(), msgs2.lock().unwrap().get(0).unwrap());

    info!("Done");
    Ok(())
}

async fn connect_test_simple() -> Result<(), String> {
    let mut ws_conn = WebSocketDummy::new();

    // First node
    let rtc_spawner = Box::new(|cs| WebRTCConnectionSetupWasm::new(cs));
    let my_storage = Box::new(DataStorageDummy {});
    let ws = Box::new(ws_conn.get_connection()?);
    let mut node1 = Node::new(my_storage, "test", ws, rtc_spawner)?;

    // Second node
    let rtc_spawner = Box::new(|cs| WebRTCConnectionSetupWasm::new(cs));
    let my_storage = Box::new(DataStorageDummy {});
    let ws = Box::new(ws_conn.get_connection()?);
    let mut node2 = Node::new(my_storage, "test", ws, rtc_spawner)?;

    // Pass messages
    ws_conn.run_queue()?;
    node1.send(&node2.info()?.id, "ping".to_string())?;

    let mut i: i32 = 0;
    loop {
        info!("Running queue: {}", i);
        node1.process().await?;
        node2.process().await?;

        i = i + 1;
        ws_conn.run_queue()?;
        // if ws_conn.run_queue()? == 0 {
        //     break;
        // }
        if i == 12 {
            info!("Connection should be set up now");
            node1.send(&node2.info()?.id, "ping".to_string())?;
            node2.send(&node1.info()?.id, "pong".to_string())?;
        }
        if i > 20 {
            break;
        }
        wait_ms(100).await;
    }

    info!("Waiting 2 seconds again");
    wait_ms(2000).await;

    Ok(())
}

async fn test_channel() -> Result<(), String> {
    let (tx, rx) = channel::<&str>();
    tx.send("one").map_err(|e| e.to_string())?;
    tx.send("two").map_err(|e| e.to_string())?;
    info!("rx is: {:?}", rx.try_iter().collect::<Vec<&str>>());
    tx.send("three").map_err(|e| e.to_string())?;
    info!("rx is: {:?}", rx.try_iter().collect::<Vec<&str>>());
    Ok(())
}

#[wasm_bindgen_test]
async fn connect_test() {
    console_error_panic_hook::set_once();

    wasm_logger::init(wasm_logger::Config::default());

    match async {
        connect_test_base().await?;
        test_channel().await?;
        connect_test_simple().await
    }
    .await
    {
        Ok(_) => info!("All OK"),
        Err(e) => info!("Something went wrong: {}", e),
    };
}

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
