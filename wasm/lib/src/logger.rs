use std::sync::Arc;
use std::sync::Mutex;use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;

use common::node::ext_interface::Logger;

use crate::logs::wait_ms;

pub struct LoggerOutput {
    pub str: Arc<Mutex<String>>,
    pub ch: Receiver<String>,
}

impl LoggerOutput {
    pub fn new() -> (LoggerOutput, NodeLogger) {
        let (send, rcv) = mpsc::sync_channel::<String>(100);
        (
            LoggerOutput {
                str: Arc::new(Mutex::new("\n".to_string())),
                ch: rcv,
            },
            NodeLogger { ch: send },
        )
    }

    pub async fn listen(ch: Receiver<String>, str: Arc<Mutex<String>>) {
        loop {
            if let Some(s) = ch.try_iter().next(){
                let mut str_copy = str.lock().unwrap();
                *str_copy = format!("\n{}{}", s, str_copy);
                continue;
            }
            wait_ms(100).await;
        }
    }
}

pub struct NodeLogger {
    ch: SyncSender<String>,
}

impl NodeLogger {
    fn print(&self, s: String){
        if let Err(e) = self.ch.send(s.clone()){
            console_log!("Coudln't send: {:?}", e);
        }
        console_log!("{:?}", s);
    }
}

impl Logger for NodeLogger {
    fn info(&self, s: &str) {
        self.print(format!("info: {:?}", s));
    }

    fn warn(&self, s: &str) {
        self.print(format!("warn: {:?}", s));
    }

    fn error(&self, s: &str) {
        self.print(format!("err : {:?}", s));
    }

    fn clone(&self) -> Box<dyn Logger>{
        Box::new(NodeLogger{ch: self.ch.clone()})
    }
}
