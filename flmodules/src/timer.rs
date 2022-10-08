use async_trait::async_trait;
use std::time::Duration;
use tokio_stream::StreamExt;

use flarch::{spawn_local, Interval};

use crate::broker::{Broker, BrokerError, Subsystem, SubsystemListener};

#[derive(Debug, Clone, PartialEq)]
pub enum TimerMessage {
    Second,
    Minute,
}

/// The Timer structure sends out periodic signals to the system so that
/// services can subscribe to them.
pub struct TimerBroker {
    seconds: u32,
}

impl TimerBroker {
    pub async fn start() -> Result<Broker<TimerMessage>, BrokerError> {
        let mut broker = Broker::new();
        let timer_struct = TimerBroker { seconds: 0 };
        broker
            .add_subsystem(Subsystem::Handler(Box::new(timer_struct)))
            .await?;
        let mut broker_cl = broker.clone();
        spawn_local(async move {
            let mut interval = Interval::new_interval(Duration::from_millis(1000));
            loop {
                interval.next().await;
                if let Err(e) = broker_cl.emit_msg(TimerMessage::Second) {
                    log::error!("While emitting timer: {e:?}");
                }
            }
        });
        Ok(broker)
    }
}

#[cfg_attr(feature = "nosend", async_trait(?Send))]
#[cfg_attr(not(feature = "nosend"), async_trait)]
impl SubsystemListener<TimerMessage> for TimerBroker {
    async fn messages(&mut self, _: Vec<TimerMessage>) -> Vec<TimerMessage> {
        if self.seconds == 0 {
            self.seconds = 59;
            return vec![TimerMessage::Minute];
        }
        self.seconds -= 1;
        vec![]
    }
}
