use flutils::{broker::Broker, time::schedule_repeating};

use crate::node::modules::messages::BrokerMessage;

#[derive(Debug, Clone, PartialEq)]
pub enum BrokerTimer {
    Second,
    Minute,
}

/// The Timer structure sends out periodic signals to the system so that
/// services can subscribe to them.
pub struct Timer {
    seconds: u32,
    broker: Broker<BrokerMessage>,
}

impl Timer {
    pub fn start(broker: Broker<BrokerMessage>) {
        let mut timer_struct = Timer { seconds: 0, broker };
        schedule_repeating(move || timer_struct.process());
    }

    fn emit(&mut self, msg: BrokerTimer) {
        if let Err(e) = self
            .broker
            .emit_msgs(vec![BrokerMessage::Timer(msg.clone())])
        {
            log::warn!("While emitting {:?}, got error: {:?}", msg, e);
        }
    }

    fn process(&mut self) {
        self.emit(BrokerTimer::Second);
        if self.seconds == 0 {
            self.emit(BrokerTimer::Minute);
            self.seconds = 60;
        }
        self.seconds -= 1;
    }
}
