use crate::broker::{Broker, Subsystem, SubsystemListener};

pub fn broker_join<R: 'static + Clone + TryFrom<S> + TryInto<S>, S: 'static + Clone>(
    broker_r: &Broker<R>,
    broker_s: &Broker<S>,
) {
    let listener_r = ListenerR{broker: broker_s.clone()};
    let listener_s = ListenerS{broker: broker_r.clone()};
    broker_r
        .clone()
        .add_subsystem(Subsystem::Handler(Box::new(listener_r)))
        .unwrap();
    broker_s
        .clone()
        .add_subsystem(Subsystem::Handler(Box::new(listener_s)))
        .unwrap();
}

pub struct ListenerR<S: Clone> {
    broker: Broker<S>,
}

impl<R: Clone + TryInto<S>, S: Clone> SubsystemListener<R> for ListenerR<S> {
    fn messages(&mut self, msgs: Vec<&R>) -> Vec<R> {
        for msg in msgs {
            if let Ok(msg_s) = msg.clone().try_into() {
                self.broker.enqueue_msg(msg_s);
            }
        }
        vec![]
    }
}

pub struct ListenerS<R: Clone> {
    broker: Broker<R>,
}

impl<R: Clone + TryFrom<S>, S: Clone> SubsystemListener<S> for ListenerS<R> {
    fn messages(&mut self, msgs: Vec<&S>) -> Vec<S> {
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

    use thiserror::Error;

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

    impl TryInto<MessageB> for MessageA {
        type Error = String;
        fn try_into(self) -> Result<MessageB, String> {
            match self {
                MessageA::Two => Ok(MessageB::Deux),
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

    #[derive(Error, Debug)]
    enum ConvertError {
        #[error("Wrong conversion")]
        Conversion(String),
        #[error(transparent)]
        Broker(#[from] BrokerError),    
    }

    #[test]
    fn convert() -> Result<(), ConvertError>{
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
            return Err(ConvertError::Conversion("A to B".to_string()));
        }
        broker_b.emit_msg(MessageB::Un)?;
        tap_b_rx.recv().unwrap();
        broker_a.process()?;
        if let Ok(msg) = tap_a_rx.try_recv(){
            assert_eq!(MessageA::One, msg);
        } else {
            return Err(ConvertError::Conversion("B to A".to_string()));
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
