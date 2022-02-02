use crate::broker::{Broker, Subsystem, SubsystemListener};

pub fn broker_join<R: 'static + Clone + TryFrom<S>, S: 'static + Clone + TryFrom<R>>(
    broker1: &Broker<R>,
    broker2: &Broker<S>,
) {
    let listener1 = Listener::new(broker2.clone());
    let listener2 = Listener::new(broker1.clone());
    broker1
        .clone()
        .add_subsystem(Subsystem::Handler(Box::new(listener1)))
        .unwrap();
    broker2
        .clone()
        .add_subsystem(Subsystem::Handler(Box::new(listener2)))
        .unwrap();
}

pub struct Listener<S: Clone> {
    broker: Broker<S>,
}

impl<S: Clone> Listener<S> {
    pub fn new(broker: Broker<S>) -> Self {
        Self { broker }
    }
}

impl<R: Clone, S: Clone + TryFrom<R>> SubsystemListener<R> for Listener<S> {
    fn messages(&mut self, msgs: Vec<&R>) -> Vec<R> {
        for msg in msgs {
            if let Ok(msg_s) = msg.clone().try_into() {
                self.broker.enqueue_msg(msg_s);
            }
        }
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;

    use crate::broker::BrokerError;

    use super::*;

    #[derive(Clone, PartialEq, Debug)]
    enum MessageA {
        One,
        Two,
        Four,
    }

    impl TryFrom<MessageB> for MessageA {
        type Error = String;
        fn try_from(msg: MessageB) -> Result<Self, String> {
            match msg {
                MessageB::Un => Ok(Self::One),
                _ => Err("unknown".to_string()),
            }
        }
    }

    #[derive(Clone, PartialEq, Debug)]
    enum MessageB {
        Un,
        Deux,
        Trois,
    }

    impl TryFrom<MessageA> for MessageB {
        type Error = String;
        fn try_from(msg: MessageA) -> Result<Self, String> {
            match msg {
                MessageA::Two => Ok(Self::Deux),
                _ => Err("unknown".to_string()),
            }
        }
    }

    #[test]
    fn convert() -> Result<(), BrokerError>{
        let mut broker_a: Broker<MessageA> = Broker::new();
        let (tap_a_tx, tap_a_rx) = channel::<MessageA>();
        broker_a.add_subsystem(Subsystem::Tap(tap_a_tx))?;
        let mut broker_b: Broker<MessageB> = Broker::new();
        let (tap_b_tx, tap_b_rx) = channel::<MessageB>();
        broker_b.add_subsystem(Subsystem::Tap(tap_b_tx))?;

        broker_join(&broker_a, &broker_b);

        broker_a.emit_msg(MessageA::Two)?;
        tap_a_rx.recv().unwrap();
        broker_b.process()?;
        if let Ok(msg) = tap_b_rx.try_recv(){
            assert_eq!(MessageB::Deux, msg);
        } else {
            return Err(BrokerError::BMDecode);
        }
        broker_b.emit_msg(MessageB::Un)?;
        tap_b_rx.recv().unwrap();
        broker_a.process()?;
        if let Ok(msg) = tap_a_rx.try_recv(){
            assert_eq!(MessageA::One, msg);
        } else {
            return Err(BrokerError::BMDecode);
        }

        broker_a.emit_msg(MessageA::Four)?;
        tap_a_rx.recv().unwrap();
        broker_b.process()?;
        assert!(tap_b_rx.try_recv().is_err());
        broker_b.emit_msg(MessageB::Trois)?;
        tap_b_rx.recv().unwrap();
        broker_a.process()?;
        assert!(tap_a_rx.try_recv().is_err());

        Ok(())
    }
}
