use common::ext_interface::Logger;


use std::sync::Arc;
use std::sync::Mutex;use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;

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

impl Logger for NodeLogger {
    fn info(&self, s: &str) {
        self.ch.send(format!("info: {:?}", s));
        console_log!("info: {:?}", s);
    }

    fn warn(&self, s: &str) {
        self.ch.send(format!("warn: {:?}", s));
        console_warn!("warn: {:?}", s);
    }

    fn error(&self, s: &str) {
        self.ch.send(format!("err : {:?}", s));
        console_warn!("err : {:?}", s);
    }
}
