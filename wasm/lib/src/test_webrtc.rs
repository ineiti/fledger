use std::cell::RefCell;
use std::rc::Rc;

use common::{node::{
    ext_interface::{DataStorage, Logger},
    Node,
    types::U256,
},
signal::{
    web_rtc::{WSSignalMessage, WebSocketMessage},
    websocket::{MessageCallback, WSMessage, WebSocketConnection},
}};

use wasm_bindgen_test::*;

use crate::logs::wait_ms;
use crate::storage_logs::ConsoleLogger;
use crate::web_rtc_setup::WebRTCConnectionSetupWasm;

struct Message {
    str: String,
    id: u32,
}

struct WebSocketDummy {
    msg_queue: Rc<RefCell<Vec<Message>>>,
    // connections: Vec<WebSocketConnectionDummy>,
    callbacks: Vec<Rc<RefCell<Option<MessageCallback>>>>,
    logger: Box<dyn Logger>,
}

impl WebSocketDummy {
    fn new(logger: Box<dyn Logger>) -> WebSocketDummy {
        WebSocketDummy {
            msg_queue: Rc::new(RefCell::new(vec![])),
            // connections: vec![],
            callbacks: vec![],
            logger,
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
            msg: WSSignalMessage::Challenge(U256::rnd()),
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
        msgs.iter().for_each(|msg| {
            if let Ok(wsm) = WebSocketMessage::from_str(&msg.str) {
                self.logger
                    .info(&format!("Got msg {:?} from {}", wsm.msg, msg.id));
                match wsm.msg {
                    WSSignalMessage::PeerSetup(_) => {
                        if self.callbacks.len() == 2 {
                            if let Err(e) = self.send_message(1 - msg.id as usize, msg.str.clone())
                            {
                                self.logger.error(&format!("couldn't push message: {}", e));
                            }
                        }
                    }
                    _ => {}
                }
            }
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

    // fn clone(&self) -> WebSocketConnectionDummy {
    //     WebSocketConnectionDummy {
    //         msg_queue: Rc::clone(&self.msg_queue),
    //         id: self.id,
    //         cb: Rc::clone(&self.cb),
    //     }
    // }
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

#[wasm_bindgen_test]
async fn connect_test() {
    console_error_panic_hook::set_once();

    let log = Box::new(ConsoleLogger {});
    match connect_test_simple().await {
        Ok(_) => log.info("All OK"),
        Err(e) => log.error(&format!("Something went wrong: {}", e)),
    };
}

async fn connect_test_simple() -> Result<(), String> {
    let log = Box::new(ConsoleLogger {});
    let mut ws_conn = WebSocketDummy::new(log.clone());

    // First node
    let rtc_spawner = Box::new(|cs| WebRTCConnectionSetupWasm::new(cs));
    let my_storage = Box::new(DataStorageDummy {});
    let ws = Box::new(ws_conn.get_connection()?);
    let node1 = Node::new(my_storage, log.clone(), ws, rtc_spawner)?;

    // Second node
    let rtc_spawner = Box::new(|cs| WebRTCConnectionSetupWasm::new(cs));
    let my_storage = Box::new(DataStorageDummy {});
    let ws = Box::new(ws_conn.get_connection()?);
    let node2 = Node::new(my_storage, log.clone(), ws, rtc_spawner)?;

    // Pass messages
    ws_conn.run_queue()?;
    node1.send(&node2.info.public, "ping".to_string()).await?;
    let mut i = 0;
    loop {
        log.info(&format!("Running queue: {}", i));
        i = i + 1;
        if ws_conn.run_queue()? == 0 {
            break;
        }
        wait_ms(1000).await;
    }
    // node1.send(&node2.info.public, "ping".to_string()).await?;
    // node2.send(&node1.info.public, "pong".to_string()).await?;
    Ok(())
}

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
