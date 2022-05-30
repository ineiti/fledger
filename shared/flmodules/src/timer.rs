use crate::broker::Broker;
use flarch::{block_on, tasks::schedule_repeating};

#[derive(Debug, Clone, PartialEq)]
pub enum TimerMessage {
    Second,
    Minute,
}

/// The Timer structure sends out periodic signals to the system so that
/// services can subscribe to them.
#[derive(Clone)]
pub struct BrokerTimer {
    seconds: u32,
    broker: Broker<TimerMessage>,
}

impl BrokerTimer {
    pub fn start() -> Broker<TimerMessage> {
        let broker = Broker::new();
        let timer_struct = BrokerTimer {
            seconds: 0,
            broker: broker.clone(),
        };
        schedule_repeating(move || {
            let mut timer_struct = timer_struct.clone();
            block_on(async move {
                timer_struct.process().await;
            });
        });
        broker
    }

    async fn emit(&mut self, msg: TimerMessage) {
        if let Err(e) = self.broker.emit_msg(msg.clone()).await {
            log::warn!("While emitting {:?}, got error: {:?}", msg, e);
        }
    }

    async fn process(&mut self) {
        self.emit(TimerMessage::Second).await;
        if self.seconds == 0 {
            self.emit(TimerMessage::Minute).await;
            self.seconds = 60;
        }
        self.seconds -= 1;
    }
}
