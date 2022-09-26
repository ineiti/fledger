use core::fmt;
use std::{
    collections::HashMap,
    fmt::Formatter,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc,
    },
};

use async_trait::async_trait;
use futures::{future::BoxFuture, lock::Mutex};
use thiserror::Error;
use tokio::sync::mpsc;

use flarch::spawn_local;

#[derive(Debug, Error)]
pub enum BrokerError {
    #[error("While sending to tap")]
    SendQueue,
    #[error("Internal structure is locked")]
    Locked,
}

pub type BrokerID = U256;

/// The Destination of the message
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    All,
    NoTap,
    Forwarded(Vec<BrokerID>),
    Handled(usize),
}

enum SubsystemAction<T> {
    Add(usize, Subsystem<T>),
    Remove(usize),
}

impl<T> fmt::Debug for SubsystemAction<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SubsystemAction::Add(id, ss) => write!(f, "Add({}, {ss:?})", id),
            SubsystemAction::Remove(id) => write!(f, "Remove({})", id),
        }
    }
}

#[cfg(feature = "nosend")]
mod asy {
    pub trait Async {}
    impl<T> Async for T {}
}
#[cfg(not(feature = "nosend"))]
mod asy {
    pub trait Async: Sync + Send {}
    impl<T: Sync + Send> Async for T {}
}
pub use asy::*;

use crate::nodeids::U256;

/// The broker connects the different subsystem together and offers
/// a pub/sub system.
/// Every subsystem can subscribe to any number of messages.
/// Every subsystem can emit any number of messages.
/// Incoming messages are queued in a channel and are treated when
/// the `process` method is called.
pub struct Broker<T: Async + Clone + fmt::Debug> {
    intern_tx: mpsc::UnboundedSender<InternMessage<T>>,
    subsystems: Arc<Mutex<usize>>,
    id: BrokerID,
}

impl<T: 'static + Async + Clone + fmt::Debug> Default for Broker<T> {
    /// Create a new broker.
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Async + Clone + fmt::Debug> Clone for Broker<T> {
    /// Clone the broker. The new broker will communicate with the same "Intern" structure
    /// and share all messages. However, each broker clone will have its own tap messages.
    fn clone(&self) -> Self {
        Self {
            intern_tx: self.intern_tx.clone(),
            subsystems: Arc::clone(&self.subsystems),
            id: self.id,
        }
    }
}

impl<T: 'static + Async + Clone + fmt::Debug> Broker<T> {
    pub fn new() -> Self {
        let id = BrokerID::rnd();
        let intern_tx = Intern::new(id);
        Self {
            intern_tx,
            subsystems: Arc::new(Mutex::new(0)),
            id,
        }
    }

    /// Adds a new subsystem to send and/or receive messages.
    pub async fn add_subsystem(&mut self, ss: Subsystem<T>) -> Result<usize, BrokerError> {
        let subsystem = {
            let mut subsystems = self.subsystems.lock().await;
            *subsystems += 1;
            *subsystems
        };
        self.intern_tx
            .send(InternMessage::Subsystem(SubsystemAction::Add(
                subsystem, ss,
            )))
            .map_err(|_| BrokerError::SendQueue)?;
        // self.settle(vec![]).await?;
        Ok(subsystem)
    }

    pub async fn remove_subsystem(&mut self, ss: usize) -> Result<(), BrokerError> {
        self.intern_tx
            .send(InternMessage::Subsystem(SubsystemAction::Remove(ss)))
            .map_err(|_| BrokerError::SendQueue)?;
        self.settle(vec![]).await
    }

    pub async fn get_tap(&mut self) -> Result<(Receiver<T>, usize), BrokerError> {
        let (tx, rx) = channel();
        let pos = self.add_subsystem(Subsystem::Tap(tx)).await?;
        Ok((rx, pos))
    }

    /// Emit a message to a given destination of other listeners.
    /// The message will be processed asynchronously.
    pub async fn emit_msg_dest(&mut self, dst: Destination, msg: T) -> Result<(), BrokerError> {
        self.intern_tx
            .send(InternMessage::Message(dst, msg))
            .map_err(|_| BrokerError::SendQueue)
    }

    /// Emit a message to other listeners.
    /// The message will be processed asynchronously.
    pub async fn emit_msg(&mut self, msg: T) -> Result<(), BrokerError> {
        self.emit_msg_dest(Destination::All, msg).await
    }

    /// Emit a message to a given destination of other listeners and wait for all involved
    /// brokers to settle.
    pub async fn settle_msg_dest(&mut self, dst: Destination, msg: T) -> Result<(), BrokerError> {
        self.intern_tx
            .send(InternMessage::Message(dst, msg))
            .map_err(|_| BrokerError::SendQueue)?;
        self.settle(vec![]).await
    }

    /// Emit a message to other listeners and wait for all involved brokers to settle.
    pub async fn settle_msg(&mut self, msg: T) -> Result<(), BrokerError> {
        self.settle_msg_dest(Destination::All, msg).await
    }

    /// Connects to another broker. The message type of the other broker
    /// needs to implement the TryFrom and TryInto for this broker's message
    /// type.
    /// Any error in the TryInto and TryFrom are interpreted as a message
    /// that cannot be translated and that is ignored.
    pub async fn link_bi<R: 'static + Async + Clone + fmt::Debug>(
        &mut self,
        mut broker: Broker<R>,
        link_rt: Translate<R, T>,
        link_tr: Translate<T, R>,
    ) -> Result<(), BrokerError> {
        self.forward(broker.clone(), link_tr).await;
        broker.forward(self.clone(), link_rt).await;
        Ok(())
    }

    /// Forwards all messages from this broker to another broker.
    /// The link_tr method is used to translate messages from this
    /// broker to the other broker.
    /// If you need a bidirectional link, use the link_bi method.
    pub async fn forward<R: 'static + Async + Clone + fmt::Debug>(
        &mut self,
        broker: Broker<R>,
        link_tr: Translate<T, R>,
    ) {
        let translator_tr = Translator {
            broker,
            translate: link_tr,
        };
        self.add_subsystem(Subsystem::Translator(Box::new(translator_tr)))
            .await
            .unwrap();
    }

    /// Waits for all messages in the queue to be forwarded / handled, before returning.
    /// It also calls all brokers that are signed up as forwarding targets.
    /// The caller argument is to be used when recursively settling, to avoid
    /// endless loops.
    async fn settle(&mut self, callers: Vec<BrokerID>) -> Result<(), BrokerError> {
        let (tx, mut rx) = mpsc::unbounded_channel();
        self.intern_tx
            .send(InternMessage::Settle(callers.clone(), tx))
            .map_err(|_| BrokerError::SendQueue)?;
        rx.recv().await;
        Ok(())
    }

    #[cfg(feature = "nosend")]
    pub fn emit_msg_wasm(&self, msg: T) {
        let mut broker = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            broker
                .emit_msg(msg)
                .await
                .err()
                .map(|e| log::error!("{:?}", e));
        });
    }
}

#[cfg(not(feature = "nosend"))]
pub type Translate<R, T> = Box<dyn Fn(R) -> Option<T> + Send + 'static>;
#[cfg(feature = "nosend")]
pub type Translate<R, T> = Box<dyn Fn(R) -> Option<T> + 'static>;

struct Translator<R: Clone, S: Async + Clone + fmt::Debug> {
    broker: Broker<S>,
    translate: Translate<R, S>,
}

#[cfg_attr(feature = "nosend", async_trait(?Send))]
#[cfg_attr(not(feature = "nosend"), async_trait)]
impl<R: Async + Clone + fmt::Debug, S: 'static + Async + Clone + fmt::Debug> SubsystemTranslator<R>
    for Translator<R, S>
{
    async fn translate(&mut self, trail: Vec<BrokerID>, msg: R) -> bool {
        let msg_res = (self.translate)(msg.clone());
        if let Some(msg_tr) = msg_res {
            log::trace!(
                "Translated {} -> {}: {msg_tr:?}, sending to {}",
                std::any::type_name::<R>(),
                std::any::type_name::<S>(),
                self.broker.id,
            );
            self.broker
                .emit_msg_dest(Destination::Forwarded(trail), msg_tr)
                .await
                .err()
                .map(|e| {
                    log::error!(
                        "{:p}: Translated message {} couldn't be queued: {e}",
                        self,
                        std::any::type_name::<S>()
                    );
                });
            return true;
        }
        false
    }

    async fn settle(&mut self, callers: Vec<BrokerID>) -> Result<(), BrokerError> {
        if !callers.contains(&self.broker.id) {
            self.broker.settle(callers).await?;
        }
        Ok(())
    }
}

enum InternMessage<T: Async + Clone + fmt::Debug> {
    Subsystem(SubsystemAction<T>),
    Message(Destination, T),
    Settle(Vec<BrokerID>, mpsc::UnboundedSender<bool>),
}

struct Intern<T: Async + Clone + fmt::Debug> {
    main_rx: mpsc::UnboundedReceiver<InternMessage<T>>,
    subsystems: HashMap<usize, Subsystem<T>>,
    msg_queue: Vec<(Destination, T)>,
    id: BrokerID,
}

impl<T: Async + Clone + fmt::Debug + 'static> Intern<T> {
    fn new(id: BrokerID) -> mpsc::UnboundedSender<InternMessage<T>> {
        log::trace!("Creating Broker {} for {}", id, std::any::type_name::<T>());
        let (main_tx, main_rx) = mpsc::unbounded_channel::<InternMessage<T>>();
        spawn_local(async move {
            let mut intern = Self {
                main_rx,
                subsystems: HashMap::new(),
                msg_queue: vec![],
                id,
            };
            loop {
                if !intern.get_msg().await {
                    log::warn!(
                        "{}: Closed Intern.main_rx for {}",
                        intern.id,
                        std::any::type_name::<T>()
                    );
                    return;
                }

                if intern.msg_queue.len() > 0 {
                    match intern.process().await {
                        Ok(nbr) => log::trace!("Processed {nbr} messages"),
                        Err(e) => log::error!("Couldn't process: {e:?}"),
                    }
                }
            }
        });

        main_tx
    }

    // The process method goes through all incoming messages and treats them from the first
    // subscribed subsystem to the last.
    // This means that it's possible if a subscription and a message are pending at the same
    // time, that you won't get what you expect.
    async fn process(&mut self) -> Result<usize, BrokerError> {
        let mut msg_count: usize = 0;
        loop {
            self.process_once().await?;
            if self.msg_queue.len() == 0 {
                break;
            }
            msg_count += self.msg_queue.len();
        }
        Ok(msg_count)
    }

    // Tries to get a new message from the channel.
    // This call blocks on purpose until a new message is available.
    // If the return value is false, then the channel has been closed.
    async fn get_msg(&mut self) -> bool {
        let msg_queue = match self.main_rx.recv().await {
            Some(msg) => msg,
            None => {
                return false;
            }
        };
        let msg = match msg_queue {
            InternMessage::Subsystem(ss) => {
                log::trace!(
                    "{self:p}/{} subsystem action {ss:?}",
                    std::any::type_name::<T>()
                );
                self.subsystem_action(ss);
                return true;
            }
            InternMessage::Message(dst, msg) => (dst, msg),
            InternMessage::Settle(list, reply) => {
                if !list.contains(&self.id) {
                    let mut list = list.clone();
                    list.push(self.id);
                    for (_, ss) in self.subsystems.iter_mut() {
                        ss.settle(list.clone())
                            .await
                            .err()
                            .map(|e| log::error!("While settling: {e:?}"));
                    }
                }
                reply
                    .send(true)
                    .err()
                    .map(|e| log::error!("Couldn't send: {e:?}"));
                return true;
            }
        };
        self.msg_queue.push(msg);

        true
    }

    /// Adds a SubsystemInit
    fn subsystem_action(&mut self, ssa: SubsystemAction<T>) {
        match ssa {
            SubsystemAction::Add(pos, s) => {
                self.subsystems.insert(pos, s);
            }
            SubsystemAction::Remove(pos) => {
                self.subsystems.remove(&pos);
            }
        }
    }

    // Goes once through all subsystems and processes the messages:
    // 1. Send all messages to the taps, except those with Destination::NoTap
    // 2. Translate all messages, and remove the translated ones
    // 3. Send all messages to the handlers, and collect messages
    async fn process_once(&mut self) -> Result<(), BrokerError> {
        self.process_translate_messages().await;

        self.send_tap().await?;

        let subsystems_error = self.process_handle_messages().await;

        for index in subsystems_error.iter().rev() {
            self.subsystem_action(SubsystemAction::Remove(*index));
        }

        Ok(())
    }

    // Then run translators on the messages. For each message, if it has been
    // translated by at least one translator, it will be deleted from the queue.
    async fn process_translate_messages(&mut self) {
        let mut i = 0;
        while i < self.msg_queue.len() {
            let (dst, msg) = &mut self.msg_queue[i];

            let mut trail = vec![];
            if let Destination::Forwarded(t) = dst.clone() {
                trail.extend(t);
            }
            if trail.contains(&self.id) {
                log::trace!("Endless forward-loop detected, aborting");
                i += 1;
                continue;
            }
            trail.push(self.id);

            let mut translated = false;
            for (_, ss) in self
                .subsystems
                .iter_mut()
                .filter(|(_, ss)| ss.is_translator())
            {
                if match ss {
                    Subsystem::Translator(ref mut translator) => {
                        translator.translate(trail.clone(), msg.clone()).await
                    }
                    Subsystem::TranslatorCallback(translator) => {
                        (translator)(trail.clone(), msg.clone()).await
                    }
                    _ => false,
                } {
                    translated = true;
                }
            }
            if translated {
                self.msg_queue.remove(i);
            } else {
                i += 1;
            }
        }
    }

    // Then send messages to all other subsystems, and collect
    // new messages for next call to 'process'.
    async fn process_handle_messages(&mut self) -> Vec<usize> {
        if self.msg_queue.len() == 0 {
            return vec![];
        }
        let mut ss_remove = vec![];
        let mut new_msg_queue = vec![];
        for (index_ss, ss) in self.subsystems.iter_mut().filter(|(_, ss)| ss.is_handler()) {
            let msgs: Vec<T> = self
                .msg_queue
                .iter()
                .filter(|nm| match nm.0 {
                    Destination::Handled(i) => index_ss != &i,
                    _ => true,
                })
                .map(|nm| &nm.1)
                .cloned()
                .collect();
            match ss.put_messages(*index_ss, msgs.clone()).await {
                Ok(mut new_msgs) => {
                    new_msg_queue.append(&mut new_msgs);
                }
                Err(e) => {
                    ss_remove.push(*index_ss);
                    log::error!("While sending messages: {}", e);
                }
            }
        }
        self.msg_queue = new_msg_queue;

        ss_remove
    }

    // Sends the messages, except Destination::NoTap, to the taps.
    async fn send_tap(&mut self) -> Result<(), BrokerError> {
        let msgs: Vec<T> = self
            .msg_queue
            .iter()
            .filter(|(dst, _)| dst != &Destination::NoTap)
            .map(|(_, msg)| msg.clone())
            .collect();

        for (i, ss) in self.subsystems.iter_mut().filter(|(_, ss)| ss.is_tap()) {
            ss.put_messages(*i, msgs.clone()).await?;
        }

        Ok(())
    }
}

#[cfg(feature = "nosend")]
pub enum Subsystem<T> {
    Tap(Sender<T>),
    Handler(Box<dyn SubsystemListener<T>>),
    Translator(Box<dyn SubsystemTranslator<T>>),
    Callback(SubsystemCallback<T>),
    TranslatorCallback(SubsystemTranslatorCallback<T>),
}
#[cfg(not(feature = "nosend"))]
pub enum Subsystem<T> {
    Tap(Sender<T>),
    Handler(Box<dyn SubsystemListener<T> + Send>),
    Translator(Box<dyn SubsystemTranslator<T> + Send>),
    Callback(SubsystemCallback<T>),
    TranslatorCallback(SubsystemTranslatorCallback<T>),
}

impl<T: Async + Clone + fmt::Debug> Subsystem<T> {
    async fn put_messages(
        &mut self,
        index: usize,
        msgs: Vec<T>,
    ) -> Result<Vec<(Destination, T)>, BrokerError> {
        Ok(match self {
            Self::Tap(s) => {
                for msg in msgs {
                    s.send(msg.clone()).map_err(|_| BrokerError::SendQueue)?;
                }
                vec![]
            }
            Self::Handler(h) => {
                let ret = h.messages(msgs).await;
                ret.into_iter()
                    .map(|m| (Destination::Handled(index), m))
                    .collect()
            }
            Self::Callback(h) => h(msgs).await,
            _ => vec![],
        })
    }

    fn is_tap(&self) -> bool {
        matches!(self, Self::Tap(_))
    }

    fn is_translator(&self) -> bool {
        matches!(self, Self::Translator(_)) || matches!(self, Self::TranslatorCallback(_))
    }

    fn is_handler(&self) -> bool {
        matches!(self, Self::Handler(_)) || matches!(self, Self::Callback(_))
    }

    async fn settle(&mut self, callers: Vec<BrokerID>) -> Result<(), BrokerError> {
        if let Self::Translator(tr) = self {
            tr.settle(callers).await
        } else {
            Ok(())
        }
    }
}

impl<T> fmt::Debug for Subsystem<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tap(_) => write!(f, "Tap"),
            Self::Handler(_) => write!(f, "Handler"),
            Self::Translator(_) => write!(f, "Translator"),
            Self::TranslatorCallback(_) => write!(f, "TranslatorCallback"),
            Self::Callback(_) => write!(f, "Callback"),
        }
    }
}

#[cfg_attr(feature = "nosend", async_trait(?Send))]
#[cfg_attr(not(feature = "nosend"), async_trait)]
pub trait SubsystemListener<T: Async> {
    async fn messages(&mut self, from_broker: Vec<T>) -> Vec<T>;
}

#[cfg_attr(feature = "nosend", async_trait(?Send))]
#[cfg_attr(not(feature = "nosend"), async_trait)]
pub trait SubsystemTranslator<T: Async> {
    async fn translate(&mut self, trail: Vec<BrokerID>, from_broker: T) -> bool;
    async fn settle(&mut self, callers: Vec<BrokerID>) -> Result<(), BrokerError>;
}

#[cfg(feature = "nosend")]
type SubsystemCallback<T> = Box<dyn Fn(Vec<T>) -> BoxFuture<'static, Vec<(Destination, T)>>>;
#[cfg(not(feature = "nosend"))]
type SubsystemCallback<T> =
    Box<dyn Fn(Vec<T>) -> BoxFuture<'static, Vec<(Destination, T)>> + Send + Sync>;
#[cfg(feature = "nosend")]
type SubsystemTranslatorCallback<T> = Box<dyn Fn(Vec<BrokerID>, T) -> BoxFuture<'static, bool>>;
#[cfg(not(feature = "nosend"))]
type SubsystemTranslatorCallback<T> =
    Box<dyn Fn(Vec<BrokerID>, T) -> BoxFuture<'static, bool> + Send + Sync>;

#[cfg(test)]
mod tests {
    use flarch::{start_logging, start_logging_filter_level};

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    pub enum BrokerTest {
        MsgA,
        MsgB,
    }

    pub struct Tps {
        reply: Vec<(BrokerTest, BrokerTest)>,
    }

    #[cfg_attr(feature = "nosend", async_trait(?Send))]
    #[cfg_attr(not(feature = "nosend"), async_trait)]
    impl SubsystemListener<BrokerTest> for Tps {
        async fn messages(&mut self, msgs: Vec<BrokerTest>) -> Vec<BrokerTest> {
            let mut output = vec![];
            log::debug!("Msgs are: {:?} - Replies are: {:?}", msgs, self.reply);

            for msg in msgs {
                if let Some(bm) = self.reply.iter().find(|repl| repl.0 == msg) {
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
        start_logging();

        let bm_a = BrokerTest::MsgA;
        let bm_b = BrokerTest::MsgB;

        let broker = &mut Broker::new();
        // Add a first subsystem that will reply 'msg_b' when it
        // receives 'msg_a'.
        broker
            .add_subsystem(Subsystem::Handler(Box::new(Tps {
                reply: vec![(bm_a.clone(), bm_b.clone())],
            })))
            .await?;
        let (tap_tx, tap) = channel::<BrokerTest>();
        broker.add_subsystem(Subsystem::Tap(tap_tx)).await?;

        // Shouldn't reply to a msg_b, so only 1 message.
        broker.settle_msg(bm_b.clone()).await?;
        assert_eq!(tap.try_iter().count(), 1);

        // Should reply to msg_a, so the tap should have 2 messages - the original
        // and the reply.
        broker.settle_msg(bm_a.clone()).await?;
        assert_eq!(tap.try_iter().count(), 2);

        // Add the same subsystem, now it should get 3 messages - the original
        // and two replies.
        broker
            .add_subsystem(Subsystem::Handler(Box::new(Tps {
                reply: vec![(bm_a.clone(), bm_b)],
            })))
            .await?;
        broker.settle_msg(bm_a).await?;
        assert_eq!(tap.try_iter().count(), 3);

        Ok(())
    }

    #[derive(Clone, PartialEq, Debug)]
    enum MessageA {
        One,
        Two,
        Four,
    }

    fn translate_ab(msg: MessageA) -> Option<MessageB> {
        match msg {
            MessageA::Two => Some(MessageB::Deux),
            _ => None,
        }
    }

    fn translate_ba(msg: MessageB) -> Option<MessageA> {
        match msg {
            MessageB::Un => Some(MessageA::One),
            _ => None,
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

    #[tokio::test]
    async fn link() -> Result<(), ConvertError> {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let mut broker_a: Broker<MessageA> = Broker::new();
        let (tap_a_tx, tap_a_rx) = channel::<MessageA>();
        broker_a.add_subsystem(Subsystem::Tap(tap_a_tx)).await?;
        let mut broker_b: Broker<MessageB> = Broker::new();
        let (tap_b_tx, tap_b_rx) = channel::<MessageB>();
        broker_b.add_subsystem(Subsystem::Tap(tap_b_tx)).await?;

        broker_b
            .link_bi(
                broker_a.clone(),
                Box::new(translate_ab),
                Box::new(translate_ba),
            )
            .await?;

        broker_a.settle_msg(MessageA::Two).await?;
        if let Ok(msg) = tap_b_rx.try_recv() {
            assert_eq!(MessageB::Deux, msg);
        } else {
            return Err(ConvertError::Conversion("A to B".to_string()));
        }

        broker_b.settle_msg(MessageB::Un).await?;
        if let Ok(msg) = tap_a_rx.try_recv() {
            assert_eq!(MessageA::One, msg);
        } else {
            return Err(ConvertError::Conversion("B to A".to_string()));
        }

        broker_a.settle_msg(MessageA::Four).await?;
        // Remove the untranslated message
        tap_a_rx.recv().unwrap();
        assert!(tap_b_rx.try_recv().is_err());
        broker_b.settle_msg(MessageB::Trois).await?;
        // Remove the untranslated message
        tap_b_rx.recv().unwrap();
        assert!(tap_a_rx.try_recv().is_err());

        Ok(())
    }

    // Test that a forwarding-loop doesn't block the system.
    #[tokio::test]
    async fn link_infinite() -> Result<(), Box<dyn std::error::Error>> {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let mut a = Broker::<MessageA>::new();
        let mut b = Broker::<MessageA>::new();

        a.link_bi(
            b.clone(),
            Box::new(|msg| Some(msg)),
            Box::new(|msg| Some(msg)),
        )
        .await?;

        let (tap_a, _) = a.get_tap().await?;
        let (tap_b, _) = b.get_tap().await?;
        a.settle_msg(MessageA::One).await?;
        assert!(tap_a.try_recv().is_ok());
        assert!(tap_b.try_recv().is_err());

        Ok(())
    }

    async fn do_something(from_broker: Vec<MessageA>) -> Vec<(Destination, MessageA)> {
        from_broker
            .iter()
            .filter(|msg| msg == &&MessageA::One)
            .map(|_| (Destination::All, MessageA::Two))
            .collect()
    }

    #[tokio::test]
    async fn test_callback() -> Result<(), Box<dyn std::error::Error>> {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let mut b = Broker::new();
        b.add_subsystem(Subsystem::Callback(Box::new(|b_in| {
            Box::pin(do_something(b_in))
        })))
        .await?;
        let tap = b.get_tap().await?;
        b.settle_msg_dest(Destination::NoTap, MessageA::One).await?;
        assert_eq!(MessageA::Two, tap.0.recv()?);
        Ok(())
    }

    async fn test_translator_cb(msg: MessageA) -> bool {
        matches!(msg, MessageA::One)
    }

    #[tokio::test]
    async fn test_translator_callback() -> Result<(), Box<dyn std::error::Error>> {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let mut b = Broker::new();
        b.add_subsystem(Subsystem::TranslatorCallback(Box::new(|_, b_in| {
            Box::pin(test_translator_cb(b_in))
        })))
        .await?;
        let tap = b.get_tap().await?;
        b.settle_msg(MessageA::One).await?;
        assert!(tap.0.try_recv().is_err());
        b.settle_msg(MessageA::Two).await?;
        assert_eq!(MessageA::Two, tap.0.recv()?);
        Ok(())
    }
}
