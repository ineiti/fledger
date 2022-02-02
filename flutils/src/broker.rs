use std::sync::{
    mpsc::{channel, Receiver, Sender},
    Arc, Mutex,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum BrokerError {
    #[error("While sending to broker")]
    SendQueue,
    #[error("While decoding BrokerMessage")]
    BMDecode,
    #[error("Internal structure is locked")]
    Locked,
}

/// The broker connects the different subsystem together and offers
/// a pub/sub system.
/// Every subsystem can subscribe to any number of messages.
/// Every subsystem can emit any number of messages.
/// Incoming messages are queued in a channel and are treated when
/// the `process` method is called.
pub struct Broker<T: Clone> {
    intern: Arc<Mutex<Intern<T>>>,
    intern_tx: Sender<T>,
}

#[allow(clippy::all)]
unsafe impl<T: Clone> Send for Broker<T> {}

impl<T: 'static + Clone> Default for Broker<T> {
    /// Create a new broker.
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> Clone for Broker<T> {
    /// Clone the broker. The new broker will communicate with the same "Intern" structure
    /// and share all messages. However, each broker clone will have its own tap messages.
    fn clone(&self) -> Self {
        Self {
            intern: Arc::clone(&self.intern),
            intern_tx: self.intern_tx.clone(),
        }
    }
}

impl<T: 'static + Clone> Broker<T> {
    pub fn new() -> Self {
        let intern = Intern::new();
        Self {
            intern_tx: intern.clone_tx(),
            intern: Arc::new(Mutex::new(intern)),
        }
    }

    /// Adds a new subsystem to send and/or receive messages.
    pub fn add_subsystem(&mut self, ss: Subsystem<T>) -> Result<(), BrokerError> {
        let mut intern = self.intern.try_lock().map_err(|_| BrokerError::Locked)?;
        intern.add_subsystem(ss);
        Ok(())
    }

    /// Try to call the process method on the underlying listener.
    pub fn process(&mut self) -> Result<usize, BrokerError> {
        let mut intern = self.intern.try_lock().map_err(|_| BrokerError::Locked)?;
        intern.process()
    }

    /// Enqueue messages to be sent to all subsystems on the next call to process.
    pub fn enqueue_msgs(&self, msgs: Vec<T>) {
        for msg in msgs.into_iter() {
            self.intern_tx.send(msg).expect("try_send");
        }
    }

    /// Enqueue a single message
    pub fn enqueue_msg(&self, msg: T) {
        self.enqueue_msgs(vec![msg]);
    }

    /// Try to emit and processes messages.
    pub fn emit_msgs(&mut self, msgs: Vec<T>) -> Result<usize, BrokerError> {
        self.enqueue_msgs(msgs);
        self.process()
    }

    /// Try to emit and process a message
    pub fn emit_msg(&mut self, msg: T) -> Result<usize, BrokerError> {
        self.emit_msgs(vec![msg])
    }

    /// Connects to another broker. The message type of the other broker
    /// needs to implement the TryFrom and TryInto for this broker's message
    /// type.
    /// Any error in the TryInto and TryFrom are interpreted as a message
    /// that cannot be translated and are ignored.
    pub fn broker_join<R: 'static + Clone + TryFrom<T> + TryInto<T>>(
        &self,
        broker_r: &Broker<R>,
    ) {
        let listener_r = Listener {
            broker: self.clone(),
        };
        let listener_s = Listener {
            broker: broker_r.clone(),
        };
        broker_r
            .clone()
            .add_subsystem(Subsystem::Handler(Box::new(listener_r)))
            .unwrap();
        self
            .clone()
            .add_subsystem(Subsystem::Handler(Box::new(listener_s)))
            .unwrap();
    }    
}

pub struct Listener<S: Clone> {
    broker: Broker<S>,
}

impl<R: Clone + TryInto<S>, S: 'static + Clone> SubsystemListener<R> for Listener<S> {
    fn messages(&mut self, msgs: Vec<&R>) -> Vec<R> {
        self.broker.enqueue_msgs(
            msgs.iter()
                .filter_map(|&msg| msg.clone().try_into().ok())
                .collect(),
        );
        let _ = self.broker.process();
        vec![]
    }
}

struct Intern<T> {
    main_tx: Sender<T>,
    subsystems: Vec<Subsystem<T>>,
    msg_queue: Vec<Vec<T>>,
}

impl<T: Clone> Intern<T> {
    pub fn new() -> Self {
        let (main_tx, main_rx) = channel::<T>();
        Self {
            main_tx,
            subsystems: vec![Subsystem::Sender(main_rx)],
            msg_queue: vec![vec![]],
        }
    }

    // Returns a clone of the transmission-queue.
    pub fn clone_tx(&self) -> Sender<T> {
        self.main_tx.clone()
    }

    /// Adds a SubsystemInit
    pub fn add_subsystem(&mut self, ss: Subsystem<T>) {
        self.subsystems.push(ss);
        self.msg_queue.push(vec![]);
    }

    /// Crappy processing to get over my (mis)-understanding of async in the context of
    /// wasm.
    /// The process method goes through all incoming messages and treats them from the first
    /// subscribed subsystem to the last.
    /// This means that it's possible if a subscription and a message are pending at the same
    /// time, that you won't get what you expect.
    pub fn process(&mut self) -> Result<usize, BrokerError> {
        let mut msg_count: usize = 0;
        let mut new_msgs: Vec<Vec<T>> = vec![];

        // First get all new messages and concat with messages from last round.
        for (index, ss) in self.subsystems.iter().enumerate() {
            let mut msgs: Vec<T> = self
                .msg_queue
                .get_mut(index)
                .expect("Get msg_queue")
                .drain(..)
                .collect();
            msgs.append(&mut ss.get_messages());
            msg_count += msgs.len();
            new_msgs.push(msgs);
        }

        // Then send messages to all other subsystems, and collect
        // new messages for next call to 'process'.
        self.msg_queue.clear();
        for (index, ss) in self.subsystems.iter_mut().enumerate() {
            let mut msg_queue = vec![];
            for (index_nm, nms) in new_msgs.iter().enumerate() {
                if index_nm == index || nms.is_empty() {
                    msg_count += nms.len();
                    continue;
                }
                msg_queue.append(&mut ss.put_messages(nms.iter().collect()));
            }
            self.msg_queue.push(msg_queue);
        }

        Ok(msg_count)
    }
}

pub enum Subsystem<T> {
    Sender(Receiver<T>),
    Tap(Sender<T>),
    Handler(Box<dyn SubsystemListener<T>>),
}

impl<T: Clone> Subsystem<T> {
    fn get_messages(&self) -> Vec<T> {
        match self {
            Self::Sender(s) => s.try_iter().collect(),
            _ => vec![],
        }
    }

    fn put_messages(&mut self, msgs: Vec<&T>) -> Vec<T> {
        match self {
            Self::Tap(s) => {
                msgs.iter().for_each(|&msg| {
                    s.send(msg.clone())
                        .map_err(|e| log::error!("While sending: {}", e))
                        .unwrap_or(())
                });
                vec![]
            }
            Self::Handler(h) => h.messages(msgs),
            _ => vec![],
        }
    }
}

pub trait SubsystemListener<T> {
    fn messages(&mut self, from_broker: Vec<&T>) -> Vec<T>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    pub enum BrokerTest {
        MsgA,
        MsgB,
    }

    pub struct Tps {
        reply: Vec<(BrokerTest, BrokerTest)>,
    }

    impl SubsystemListener<BrokerTest> for Tps {
        fn messages(&mut self, msgs: Vec<&BrokerTest>) -> Vec<BrokerTest> {
            let mut output = vec![];
            log::debug!("Msgs are: {:?} - Replies are: {:?}", msgs, self.reply);

            for msg in msgs {
                if let Some(bm) = self.reply.iter().find(|repl| &repl.0 == msg) {
                    log::debug!("Found message");
                    output.push(bm.1.clone());
                }
            }
            output
        }
    }

    /// Test the broker with two subsystems.
    #[tokio::test]
    async fn test_broker_new() -> Result<(), BrokerError> {
        flexi_logger::Logger::try_with_str("debug").unwrap();

        let bm_a = BrokerTest::MsgA;
        let bm_b = BrokerTest::MsgB;

        let broker = &mut Broker::new();
        // Add a first subsystem that will reply 'msg_b' when it
        // receives 'msg_a'.
        broker.add_subsystem(Subsystem::Handler(Box::new(Tps {
            reply: vec![(bm_a.clone(), bm_b.clone())],
        })))?;
        let (tap_tx, tap) = channel::<BrokerTest>();
        broker.add_subsystem(Subsystem::Tap(tap_tx))?;

        // Shouldn't reply to a msg_b, so only 1 message.
        broker.emit_msgs(vec![bm_b.clone()])?;
        assert_eq!(tap.try_iter().count(), 1);

        // Should reply to msg_a, so the tap should have 2 messages - the original
        // and the reply.
        broker.emit_msgs(vec![bm_a.clone()])?;
        broker.process()?;
        assert_eq!(tap.try_iter().count(), 2);

        // Add the same subsystem, now it should get 3 messages - the original
        // and two replies.
        broker.add_subsystem(Subsystem::Handler(Box::new(Tps {
            reply: vec![(bm_a.clone(), bm_b)],
        })))?;
        broker.emit_msgs(vec![bm_a])?;
        broker.process()?;
        assert_eq!(tap.try_iter().count(), 3);

        Ok(())
    }

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
    fn convert() -> Result<(), ConvertError> {
        let mut broker_a: Broker<MessageA> = Broker::new();
        let (tap_a_tx, tap_a_rx) = channel::<MessageA>();
        broker_a.add_subsystem(Subsystem::Tap(tap_a_tx))?;
        let mut broker_b: Broker<MessageB> = Broker::new();
        let (tap_b_tx, tap_b_rx) = channel::<MessageB>();
        broker_b.add_subsystem(Subsystem::Tap(tap_b_tx))?;

        broker_b.broker_join(&broker_a);

        broker_a.emit_msg(MessageA::Two)?;
        tap_a_rx.recv().unwrap();
        broker_b.process()?;
        if let Ok(msg) = tap_b_rx.try_recv() {
            assert_eq!(MessageB::Deux, msg);
        } else {
            return Err(ConvertError::Conversion("A to B".to_string()));
        }
        broker_b.emit_msg(MessageB::Un)?;
        tap_b_rx.recv().unwrap();
        broker_a.process()?;
        if let Ok(msg) = tap_a_rx.try_recv() {
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
