///! Simple template
/// This implements a simple broker with one input and one output message.
/// It counts the number of times it has been called, and returns every
/// even sum.
/// It allows to either create a broker, or a structure which also has
/// some state which updates using the [tokio::sync::watch] channel.
use anyhow::Result;
use flarch::{
    broker::{Broker, SubsystemHandler},
    platform_async_trait,
    tasks::spawn_local,
};
use tokio::sync::watch;

/// The messages this broker takes as input.
#[derive(Debug, Clone, PartialEq)]
pub enum SimpleIn {
    Count,
}

/// The messages this broker outputs.
#[derive(Debug, Clone, PartialEq)]
pub enum SimpleOut {
    Sum(usize),
}

/// For other brokers who want to connect to this broker.
pub type SimpleBroker = Broker<SimpleIn, SimpleOut>;

/// The main structure which implements the constructors.
pub struct Simple {
    counter: usize,
}

/// A structure containing one or more states which are automatically updated.
pub struct SimpleState {
    /// The new state of the template.
    pub state: watch::Receiver<usize>,
    /// A broker to be used in other brokers.
    pub broker: SimpleBroker,
}

impl Simple {
    /// Returns a [SimpleState] and starts the broker.
    /// It uses a tap to update the state field of the structure.
    pub async fn state() -> Result<SimpleState> {
        let (tx, state) = watch::channel(0);
        let mut state = SimpleState {
            state,
            broker: Self::broker().await?,
        };

        let mut tap = state.broker.get_tap_out().await?.0;
        spawn_local(async move {
            while let Some(msg) = tap.recv().await {
                match msg {
                    SimpleOut::Sum(sum) => tx.send(sum).expect("Updating sum"),
                }
            }
        });

        Ok(state)
    }

    /// Just returns the broker for this simple template.
    pub async fn broker() -> Result<SimpleBroker> {
        let broker = Broker::new_with_handler(Box::new(Simple { counter: 0 })).await?;
        Ok(broker.0)
    }

    // Internal processing of the incoming messages.
    fn process_msg(&mut self, msg: SimpleIn) -> Option<SimpleOut> {
        match msg {
            SimpleIn::Count => {
                self.counter += 1;
                (self.counter % 2 == 0).then(|| SimpleOut::Sum(self.counter))
            }
        }
    }
}

#[platform_async_trait()]
impl SubsystemHandler<SimpleIn, SimpleOut> for Simple {
    // The message handler necessary for implementing a broker.
    async fn messages(&mut self, msgs: Vec<SimpleIn>) -> Vec<SimpleOut> {
        msgs.into_iter()
            .flat_map(|msg| self.process_msg(msg))
            .collect()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    // Testing simple updates of the counter.
    async fn test_update() -> Result<()> {
        let mut broker = Simple::broker().await?;
        let mut tap = broker.get_tap_out().await?;

        broker.emit_msg_in(SimpleIn::Count)?;
        broker.emit_msg_in(SimpleIn::Count)?;
        assert_eq!(Some(SimpleOut::Sum(2)), tap.0.recv().await);
        Ok(())
    }

    #[tokio::test]
    // Testing that the state gets updated correctly.
    async fn test_state() -> Result<()> {
        let mut state = Simple::state().await?;
        let mut tap = state.broker.get_tap_out().await?;

        state.broker.emit_msg_in(SimpleIn::Count)?;
        assert_eq!(0, *state.state.borrow());
        state.broker.emit_msg_in(SimpleIn::Count)?;
        tap.0.recv().await;
        assert_eq!(2, *state.state.borrow());

        Ok(())
    }
}
