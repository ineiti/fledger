use crate::node::modules::gossip_chat::GossipMessage;
use crate::node::modules::random_connections::RandomMessage;
use crate::node::{logic::messages::NodeMessage, network::BrokerNetwork, timer::BrokerTimer};
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

pub trait SubsystemListener {
    fn messages(&mut self, from_broker: Vec<&BrokerMessage>) -> Vec<BInput>;
}

pub trait SubsystemInit: SubsystemListener {
    fn init(&mut self, to_broker: Sender<BInput>);
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModulesMessage {
    Gossip(GossipMessage),
    Random(RandomMessage),
}

#[allow(clippy::large_enum_variant)] 
#[derive(Debug, Clone, PartialEq)]
pub enum BrokerMessage {
    Network(BrokerNetwork),
    Timer(BrokerTimer),
    NodeMessage(NodeMessage),
    Modules(ModulesMessage),
    #[cfg(test)]
    TestMessages(tests::TestMessages),
}

impl std::fmt::Display for BrokerMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BrokerMessage({})",
            match self {
                BrokerMessage::Network(_) => "Network",
                BrokerMessage::Timer(_) => "Timer",
                BrokerMessage::NodeMessage(_) => "NodeMessage",
                BrokerMessage::Modules(_) => "Modules",
                #[cfg(test)]
                BrokerMessage::TestMessages(_) => "TestMessages",
            }
        )
    }
}

#[allow(clippy::large_enum_variant)] 
#[derive(Debug, Clone)]
pub enum BInput {
    BM(BrokerMessage),
    Subscribe(Vec<String>),
}

/// The broker connects the different subsystem together and offers
/// a pub/sub system.
/// Every subsystem can subscribe to any number of messages.
/// Every subsystem can emit any number of messages.
/// Incoming messages are queued in a channel and are treated when
/// the `process` method is called.
pub struct Broker {
    intern: Arc<Mutex<Intern>>,
    intern_tx: Sender<BInput>,
}

#[allow(clippy::all)] 
unsafe impl Send for Broker {}

impl Default for Broker {
    /// Create a new broker.
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Broker {
    /// Clone the broker. The new broker will communicate with the same "Intern" structure
    /// and share all messages. However, each broker clone will have its own tap messages
    /// and is able to filter according to different messages.
    fn clone(&self) -> Self {
        Self {
            intern: Arc::clone(&self.intern),
            intern_tx: self.intern_tx.clone(),
        }
    }
}

impl Broker {
    pub fn new() -> Self {
        let intern = Intern::new();
        Self {
            intern_tx: intern.clone_tx(),
            intern: Arc::new(Mutex::new(intern)),
        }
    }

    /// Returns a Sender for messages into the broker.
    pub fn clone_tx(&self) -> Sender<BInput> {
        self.intern_tx.clone()
    }

    /// Adds a new subsystem to send and/or receive messages.
    pub fn add_subsystem(&mut self, ss: Subsystem) -> Result<(), BrokerError> {
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
    pub fn enqueue(&self, msgs: Vec<BInput>) {
        for msg in msgs.into_iter() {
            self.intern_tx.send(msg).expect("try_send");
        }
    }

    /// Enqueue a single BrokerMessageStruct
    pub fn enqueue_bm(&self, msg: BrokerMessage) {
        self.enqueue(vec![BInput::BM(msg)]);
    }

    /// Try to emit and processes messages.
    pub fn emit(&mut self, msgs: Vec<BInput>) -> Result<usize, BrokerError> {
        self.enqueue(msgs);
        self.process()
    }

    /// Try to emit and process a BrokerMessage
    pub fn emit_bm(&mut self, msg: BrokerMessage) -> Result<usize, BrokerError> {
        self.emit(vec![BInput::BM(msg)])
    }

    /// Returns a clone of the input_tx queue.
    pub fn get_input_tx(&self) -> Sender<BInput> {
        self.intern_tx.clone()
    }
}

struct Intern {
    main_tx: Sender<BInput>,
    subsystems: Vec<Subsystem>,
    msg_queue: Vec<Vec<BInput>>,
}

impl Intern {
    pub fn new() -> Self {
        let (main_tx, main_rx) = channel::<BInput>();
        Self {
            main_tx,
            subsystems: vec![Subsystem::Sender(main_rx)],
            msg_queue: vec![vec![]],
        }
    }

    // Returns a clone of the transmission-queue.
    pub fn clone_tx(&self) -> Sender<BInput> {
        self.main_tx.clone()
    }

    /// Adds a SubsystemInit
    pub fn add_subsystem(&mut self, ss: Subsystem) {
        if let Subsystem::InitHandler(mut init) = ss {
            init.init(self.main_tx.clone());
            self.subsystems.push(Subsystem::InitHandler(init));
        } else {
            self.subsystems.push(ss);
        }
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
        let mut new_msgs: Vec<Vec<BInput>> = vec![];

        // First get all new messages and concat with messages from last round.
        for (index, ss) in self.subsystems.iter().enumerate() {
            let mut msgs: Vec<BInput> = self
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
                msg_queue.append(&mut ss.put_messages(nms));
            }
            self.msg_queue.push(msg_queue);
        }

        Ok(msg_count)
    }
}

pub enum Subsystem {
    Sender(Receiver<BInput>),
    Tap(Sender<BInput>),
    InitHandler(Box<dyn SubsystemInit>),
    Handler(Box<dyn SubsystemListener>),
}

impl Subsystem {
    fn get_messages(&self) -> Vec<BInput> {
        match self {
            Self::Sender(s) => s.try_iter().collect(),
            _ => vec![],
        }
    }

    fn put_messages(&mut self, msgs: &[BInput]) -> Vec<BInput> {
        match self {
            Self::Tap(s) => {
                msgs.iter().for_each(|msg| {
                    s.send(msg.clone())
                        .map_err(|e| log::error!("While sending: {}", e))
                        .unwrap_or(())
                });
                vec![]
            }
            // TODO: find out how to write a common code for this. As the two handlers
            // are a different trait, I cannot get it to work.
            // This might be helpful:
            // https://users.rust-lang.org/t/casting-traitobject-to-super-trait/33524/11
            Self::InitHandler(h) => h.messages(
                msgs.iter()
                    .filter_map(|msg| match msg {
                        BInput::BM(bm) => Some(bm),
                        _ => None,
                    })
                    .collect(),
            ),
            Self::Handler(h) => h.messages(
                msgs.iter()
                    .filter_map(|msg| match msg {
                        BInput::BM(bm) => Some(bm),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use flexi_logger::LevelFilter;

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    pub enum TestMessages {
        MsgA,
        MsgB,
    }

    pub struct Tps {
        init_msgs: Vec<BInput>,
        reply: Vec<(BrokerMessage, BrokerMessage)>,
    }

    impl SubsystemInit for Tps {
        fn init(&mut self, to_broker: Sender<BInput>) {
            for msg in self.init_msgs.iter() {
                to_broker.send(msg.clone()).unwrap();
            }
        }
    }

    impl SubsystemListener for Tps {
        fn messages(&mut self, msgs: Vec<&BrokerMessage>) -> Vec<BInput> {
            let mut output = vec![];
            log::debug!("Msgs are: {:?} - Replies are: {:?}", msgs, self.reply);

            for msg in msgs {
                if let Some(bm) = self.reply.iter().find(|repl| &repl.0 == msg) {
                    log::debug!("Found message");
                    output.push(BInput::BM(bm.1.clone()));
                }
            }
            output
        }
    }

    /// Test the broker with two subsystems.
    #[tokio::test]
    async fn test_broker_new() -> Result<(), BrokerError> {
        simple_logging::log_to_stderr(LevelFilter::Trace);

        let bm_a = BrokerMessage::TestMessages(TestMessages::MsgA);
        let bm_b = BrokerMessage::TestMessages(TestMessages::MsgB);

        let broker = &mut Broker::new();
        // Add a first subsystem that will reply 'msg_b' when it
        // receives 'msg_a'.
        broker.add_subsystem(Subsystem::InitHandler(Box::new(Tps {
            init_msgs: vec![],
            reply: vec![(bm_a.clone(), bm_b.clone())],
        })))?;
        let (tap_tx, tap) = channel::<BInput>();
        broker.add_subsystem(Subsystem::Tap(tap_tx))?;

        // Shouldn't reply to a msg_b, so only 1 message.
        broker.emit(vec![BInput::BM(bm_b.clone())])?;
        assert_eq!(tap.try_iter().count(), 1);

        // Should reply to msg_a, so the tap should have 2 messages - the original
        // and the reply.
        broker.emit(vec![BInput::BM(bm_a.clone())])?;
        broker.process()?;
        assert_eq!(tap.try_iter().count(), 2);

        // Add the same subsystem, now it should get 3 messages - the original
        // and two replies.
        broker.add_subsystem(Subsystem::InitHandler(Box::new(Tps {
            init_msgs: vec![],
            reply: vec![(bm_a.clone(), bm_b)],
        })))?;
        broker.emit(vec![BInput::BM(bm_a)])?;
        broker.process()?;
        assert_eq!(tap.try_iter().count(), 3);

        Ok(())
    }
}
