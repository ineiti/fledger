use std::time::Duration;
use tokio_stream::StreamExt;

use flarch::broker::{Broker, Message};
use flarch::tasks::{spawn_local, Interval};

#[derive(Debug, Clone, PartialEq)]
pub enum TimerMessage {
    Second,
    Minute,
}

pub type BrokerTimer = Broker<(), TimerMessage>;

/// The Timer structure sends out periodic signals to the system so that
/// services can subscribe to them.
#[derive(Clone, Debug)]
pub struct Timer {
    pub broker: BrokerTimer,
}

impl Timer {
    pub async fn start() -> anyhow::Result<Timer> {
        let broker = Broker::new();
        let mut broker_cl = broker.clone();
        spawn_local(async move {
            let mut seconds = 0;
            let mut interval = Interval::new_interval(Duration::from_secs(1));
            loop {
                interval.next().await;
                seconds += 1;
                broker_cl
                    .emit_msg_out(TimerMessage::Second)
                    .err()
                    .map(|e| log::error!("While emitting timer: {e:?}"));
                if seconds == 60 {
                    broker_cl
                        .emit_msg_out(TimerMessage::Minute)
                        .err()
                        .map(|e| log::error!("While emitting timer: {e:?}"));
                    seconds = 0;
                }
            }
        });
        Ok(Timer { broker })
    }

    pub fn simul() -> Timer {
        Timer {
            broker: Broker::new(),
        }
    }

    pub async fn tick<I: Message + 'static, O: Message + 'static>(
        &mut self,
        broker: Broker<I, O>,
        msg: I,
        tick: TimerMessage,
    ) -> anyhow::Result<()> {
        self.broker
            .add_translator_o_ti(
                broker,
                Box::new(move |tm| (tm == tick).then_some(msg.clone())),
            )
            .await?;

        Ok(())
    }

    pub async fn tick_second<I: Message + 'static, O: Message + 'static>(
        &mut self,
        broker: Broker<I, O>,
        msg: I,
    ) -> anyhow::Result<()> {
        self.tick(broker, msg, TimerMessage::Second).await
    }

    pub async fn tick_minute<I: Message + 'static, O: Message + 'static>(
        &mut self,
        broker: Broker<I, O>,
        msg: I,
    ) -> anyhow::Result<()> {
        self.tick(broker, msg, TimerMessage::Minute).await
    }
}
