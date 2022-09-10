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
use futures::{channel::mpsc, future::BoxFuture, lock::Mutex, SinkExt};
use thiserror::Error;

use flarch::tasks::wait_ms;

#[derive(Debug, Error)]
pub enum BrokerError {
    #[error("While sending to tap")]
    SendQueue,
    #[error("Internal structure is locked")]
    Locked,
    #[error(transparent)]
    SendMPSC(#[from] mpsc::SendError),
}

pub type BrokerID = U256;

/// The Destination of the message
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    All,
    Others,
    This,
    NoTap,
    Forward(Vec<BrokerID>),
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
    intern: Arc<Mutex<Intern<T>>>,
    intern_tx: mpsc::UnboundedSender<(Destination, T)>,
    subsystem_tx: mpsc::UnboundedSender<SubsystemAction<T>>,
    subsystems: Arc<Mutex<usize>>,
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
            intern: Arc::clone(&self.intern),
            intern_tx: self.intern_tx.clone(),
            subsystem_tx: self.subsystem_tx.clone(),
            subsystems: Arc::clone(&self.subsystems),
        }
    }
}

impl<T: 'static + Async + Clone + fmt::Debug> Broker<T> {
    pub fn new() -> Self {
        let (subsystem_tx, subsystem_rx) = mpsc::unbounded();
        let intern = Intern::new(subsystem_rx);
        Self {
            intern_tx: intern.clone_tx(),
            intern: Arc::new(Mutex::new(intern)),
            subsystem_tx,
            subsystems: Arc::new(Mutex::new(0)),
        }
    }

    /// Adds a new subsystem to send and/or receive messages.
    pub async fn add_subsystem(&mut self, ss: Subsystem<T>) -> Result<usize, BrokerError> {
        let subsystem = {
            let mut subsystems = self.subsystems.lock().await;
            *subsystems += 1;
            *subsystems
        };
        self.subsystem_tx
            .send(SubsystemAction::Add(subsystem, ss))
            .await?;
        self.process()
            .await
            .err()
            .map(|e| log::trace!("While calling process: {e:?}"));
        Ok(subsystem)
    }

    pub async fn remove_subsystem(&mut self, ss: usize) -> Result<(), BrokerError> {
        self.subsystem_tx.send(SubsystemAction::Remove(ss)).await?;
        self.process_retry(10).await?;
        Ok(())
    }

    pub async fn get_tap(&mut self) -> Result<(Receiver<T>, usize), BrokerError> {
        let (tx, rx) = channel();
        let pos = self.add_subsystem(Subsystem::Tap(tx)).await?;
        Ok((rx, pos))
    }

    /// Try to call the process method on the underlying listener.
    /// If retries < 0, it will try forever.
    pub async fn process_retry(&mut self, mut retries: i32) -> Result<usize, BrokerError> {
        let mut intern = self.intern.try_lock();
        while intern.is_none() && retries != 0 {
            if retries > 0 {
                retries -= 1;
            }
            log::trace!("Couldn't lock intern - trying again in 100ms");
            wait_ms(100).await;
            intern = self.intern.try_lock();
        }

        if let Some(mut i) = intern {
            return i.process().await;
        }
        log::trace!(
            "Couldn't lock intern for {} - stop trying",
            std::any::type_name::<T>()
        );

        Err(BrokerError::Locked)
    }

    pub async fn process(&mut self) -> Result<usize, BrokerError> {
        self.process_retry(0).await
    }

    /// Enqueue a message to a given destination of other listeners.
    pub async fn enqueue_msg_dest(&mut self, dst: Destination, msg: T) -> Result<(), BrokerError> {
        self.intern_tx.send((dst, msg)).await?;
        Ok(())
    }

    /// Emit a message to a given destination of other listeners.
    pub async fn emit_msg_dest(
        &mut self,
        retries: i32,
        dst: Destination,
        msg: T,
    ) -> Result<usize, BrokerError> {
        self.intern_tx.send((dst, msg)).await.unwrap();
        self.process_retry(retries).await
    }

    /// Enqueue a single message to other listeners.
    pub async fn enqueue_msg(&mut self, msg: T) -> Result<(), BrokerError> {
        self.enqueue_msg_dest(Destination::Others, msg).await
    }

    /// Try to emit and process a message to other listeners.
    pub async fn emit_msg(&mut self, msg: T) -> Result<usize, BrokerError> {
        self.emit_msg_dest(10, Destination::Others, msg).await
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
    ) {
        self.forward(broker.clone(), link_tr).await;
        broker.forward(self.clone(), link_rt).await;
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
                "Translated {} -> {}: {msg_tr:?}",
                std::any::type_name::<R>(),
                std::any::type_name::<S>()
            );
            self.broker
                .emit_msg_dest(0, Destination::Forward(trail), msg_tr)
                .await
                .err()
                .map(|e| {
                    log::trace!(
                        "{:p}: Translated message {} queued, but not processed: {e}",
                        self,
                        std::any::type_name::<S>()
                    );
                });
            return true;
        }
        false
    }
}

struct Intern<T: Async + Clone + fmt::Debug> {
    main_tx: mpsc::UnboundedSender<(Destination, T)>,
    subsystems: HashMap<usize, Subsystem<T>>,
    msg_queue: HashMap<usize, Vec<(Destination, T)>>,
    subsystem_rx: mpsc::UnboundedReceiver<SubsystemAction<T>>,
    id: BrokerID,
}

impl<T: Async + Clone + fmt::Debug> Intern<T> {
    fn new(subsystem_rx: mpsc::UnboundedReceiver<SubsystemAction<T>>) -> Self {
        let (main_tx, main_rx) = mpsc::unbounded::<(Destination, T)>();
        let mut subsystems = HashMap::new();
        subsystems.insert(0, Subsystem::ChannelFuture(main_rx));
        let mut msg_queue = HashMap::new();
        msg_queue.insert(0, Vec::new());
        Self {
            main_tx,
            subsystems,
            msg_queue,
            subsystem_rx,
            id: U256::rnd(),
        }
    }

    // Returns a clone of the transmission-queue.
    fn clone_tx(&self) -> mpsc::UnboundedSender<(Destination, T)> {
        self.main_tx.clone()
    }

    /// Adds a SubsystemInit
    fn subsystem_action(&mut self, ssa: SubsystemAction<T>) {
        match ssa {
            SubsystemAction::Add(pos, s) => {
                self.subsystems.insert(pos, s);
                self.msg_queue.insert(pos, vec![]);
            }
            SubsystemAction::Remove(pos) => {
                self.subsystems.remove(&pos);
                self.msg_queue.remove(&pos);
            }
        }
    }

    /// Crappy processing to get over my (mis)-understanding of async in the context of
    /// wasm.
    /// The process method goes through all incoming messages and treats them from the first
    /// subscribed subsystem to the last.
    /// This means that it's possible if a subscription and a message are pending at the same
    /// time, that you won't get what you expect.
    async fn process(&mut self) -> Result<usize, BrokerError> {
        let mut msg_count: usize = 0;
        loop {
            let msgs = self.process_once().await?;
            msg_count += msgs;
            if msgs == 0 && self.msg_queue_len() == 0 {
                break;
            }
        }
        Ok(msg_count)
    }

    async fn process_once(&mut self) -> Result<usize, BrokerError> {
        while let Ok(Some(ss)) = self.subsystem_rx.try_next() {
            log::trace!(
                "{self:p}/{} subsystem action {ss:?}",
                std::any::type_name::<T>()
            );
            self.subsystem_action(ss);
        }

        let (msg_count, new_msgs) = self.process_get_new_messages().await?;

        let new_msgs = self.process_translate_messages(new_msgs).await?;

        let subsystems_remove = self.process_handle_messages(new_msgs).await;

        for index in subsystems_remove.iter().rev() {
            self.subsystem_action(SubsystemAction::Remove(*index));
        }

        Ok(msg_count)
    }

    async fn process_get_new_messages(
        &mut self,
    ) -> Result<(usize, HashMap<usize, Vec<(Destination, T)>>), BrokerError> {
        let mut msg_count: usize = 0;
        let mut new_msgs = HashMap::new();

        // First get all new messages and concat with messages from last round.
        for (index, ss) in self.subsystems.iter_mut() {
            let mut msgs: Vec<(Destination, T)> = self
                .msg_queue
                .get_mut(index)
                .expect("Get msg_queue")
                .drain(..)
                .collect();
            msgs.append(&mut ss.get_messages().await);
            msg_count += msgs.len();
            new_msgs.insert(*index, msgs);
        }
        self.msg_queue.clear();

        Ok((msg_count, new_msgs))
    }

    async fn process_translate_messages(
        &mut self,
        mut new_msgs: HashMap<usize, Vec<(Destination, T)>>,
    ) -> Result<HashMap<usize, Vec<(Destination, T)>>, BrokerError> {
        // Then run translators on the messages. For each message, if it has been
        // translated by at least one translator, it will be deleted from the queue.
        for (_, nms) in new_msgs.iter_mut() {
            let mut i = 0;
            while i < nms.len() {
                let mut trail = vec![];
                if let Destination::Forward(t) = nms[i].0.clone() {
                    trail.extend(t);
                }
                if trail.contains(&self.id) {
                    i += 1;
                    log::trace!("Endless forward-loop detected, aborting");
                    continue;
                }
                trail.push(self.id);
                let mut translated = false;
                for (_, ss) in self.subsystems.iter_mut() {
                    match ss {
                        Subsystem::Translator(ref mut translator) => {
                            translated |=
                                translator.translate(trail.clone(), nms[i].1.clone()).await
                        }
                        Subsystem::TranslatorCallback(translator) => {
                            translated |= (translator)(trail.clone(), nms[i].1.clone()).await
                        }
                        _ => {}
                    }
                }
                if translated {
                    nms.remove(i);
                } else {
                    i += 1;
                }
            }
        }

        Ok(new_msgs)
    }

    async fn process_handle_messages(
        &mut self,
        queued_msgs: HashMap<usize, Vec<(Destination, T)>>,
    ) -> Vec<usize> {
        // Then send messages to all other subsystems, and collect
        // new messages for next call to 'process'.
        let mut ss_remove = vec![];
        for (index, ss) in self.subsystems.iter_mut() {
            let mut msg_queue = vec![];
            if !ss.is_translator() {
                for (index_nm, nms) in queued_msgs.iter() {
                    if nms.len() == 0 {
                        continue;
                    }
                    let msgs = nms
                        .iter()
                        .filter(|nm| match nm.0 {
                            Destination::All | Destination::Forward(_) => true,
                            Destination::Others => index_nm != index,
                            Destination::This => index_nm == index,
                            Destination::NoTap => !ss.is_tap(),
                        })
                        .map(|nm| &nm.1)
                        .cloned()
                        .collect();
                    match ss.put_messages(msgs).await {
                        Ok(mut new_msgs) => msg_queue.append(&mut new_msgs),
                        Err(e) => {
                            ss_remove.push(*index);
                            log::error!("While sending messages: {}", e);
                        }
                    }
                }
            }
            self.msg_queue.insert(*index, msg_queue);
        }

        ss_remove
    }

    fn msg_queue_len(&self) -> usize {
        self.msg_queue.iter().fold(0, |l, (_, q)| l + q.len())
    }
}

#[cfg(feature = "nosend")]
pub enum Subsystem<T> {
    ChannelFuture(mpsc::UnboundedReceiver<(Destination, T)>),
    Channel(Receiver<(Destination, T)>),
    Tap(Sender<T>),
    Handler(Box<dyn SubsystemListener<T>>),
    Translator(Box<dyn SubsystemTranslator<T>>),
    Callback(SubsystemCallback<T>),
    TranslatorCallback(SubsystemTranslatorCallback<T>),
}
#[cfg(not(feature = "nosend"))]
pub enum Subsystem<T> {
    ChannelFuture(mpsc::UnboundedReceiver<(Destination, T)>),
    Channel(Receiver<(Destination, T)>),
    Tap(Sender<T>),
    Handler(Box<dyn SubsystemListener<T> + Send>),
    Translator(Box<dyn SubsystemTranslator<T> + Send>),
    Callback(SubsystemCallback<T>),
    TranslatorCallback(SubsystemTranslatorCallback<T>),
}

impl<T: Async + Clone + fmt::Debug> Subsystem<T> {
    async fn get_messages(&mut self) -> Vec<(Destination, T)> {
        match self {
            Self::Channel(s) => {
                let msgs = s.try_iter().collect();
                msgs
            }
            Self::ChannelFuture(s) => {
                let mut msgs = vec![];
                while let Ok(Some(msg)) = s.try_next() {
                    msgs.push(msg);
                }
                msgs
            }
            _ => vec![],
        }
    }

    async fn put_messages(&mut self, msgs: Vec<T>) -> Result<Vec<(Destination, T)>, BrokerError> {
        Ok(match self {
            Self::Tap(s) => {
                for msg in msgs {
                    s.send(msg.clone()).map_err(|_| BrokerError::SendQueue)?;
                }
                vec![]
            }
            Self::Handler(h) => {
                let ret = h.messages(msgs).await;
                ret.into_iter().map(|m| (Destination::Others, m)).collect()
            }
            Self::Callback(h) => h(msgs).await,
            _ => vec![],
        })
    }

    fn is_tap(&self) -> bool {
        matches!(self, Self::Tap(_))
    }

    fn is_translator(&self) -> bool {
        matches!(self, Self::Translator(_))
    }
}

impl<T> fmt::Debug for Subsystem<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Channel(_) => write!(f, "Channel"),
            Self::ChannelFuture(_) => write!(f, "ChannelFuture"),
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
    use flarch::start_logging;

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
        broker.emit_msg(bm_b.clone()).await?;
        assert_eq!(tap.try_iter().count(), 1);

        // Should reply to msg_a, so the tap should have 2 messages - the original
        // and the reply.
        broker.emit_msg(bm_a.clone()).await?;
        broker.process().await?;
        assert_eq!(tap.try_iter().count(), 2);

        // Add the same subsystem, now it should get 3 messages - the original
        // and two replies.
        broker
            .add_subsystem(Subsystem::Handler(Box::new(Tps {
                reply: vec![(bm_a.clone(), bm_b)],
            })))
            .await?;
        broker.emit_msg(bm_a).await?;
        broker.process().await?;
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
        start_logging();

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
            .await;

        broker_a.emit_msg(MessageA::Two).await?;
        if let Ok(msg) = tap_b_rx.try_recv() {
            assert_eq!(MessageB::Deux, msg);
        } else {
            return Err(ConvertError::Conversion("A to B".to_string()));
        }

        broker_b.emit_msg(MessageB::Un).await?;
        if let Ok(msg) = tap_a_rx.try_recv() {
            assert_eq!(MessageA::One, msg);
        } else {
            return Err(ConvertError::Conversion("B to A".to_string()));
        }

        broker_a.emit_msg(MessageA::Four).await?;
        // Remove the untranslated message
        tap_a_rx.recv().unwrap();
        assert!(tap_b_rx.try_recv().is_err());
        broker_b.emit_msg(MessageB::Trois).await?;
        // Remove the untranslated message
        tap_b_rx.recv().unwrap();
        assert!(tap_a_rx.try_recv().is_err());

        Ok(())
    }

    // Test that a forwarding-loop doesn't block the system.
    #[tokio::test]
    async fn link_infinite() -> Result<(), Box<dyn std::error::Error>> {
        start_logging();

        let mut a = Broker::<MessageA>::new();
        let mut b = Broker::<MessageA>::new();

        a.link_bi(
            b.clone(),
            Box::new(|msg| Some(msg)),
            Box::new(|msg| Some(msg)),
        )
        .await;

        let (tap_a, _) = a.get_tap().await?;
        let (tap_b, _) = b.get_tap().await?;
        a.emit_msg(MessageA::One).await?;
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
        let mut b = Broker::new();
        b.add_subsystem(Subsystem::Callback(Box::new(|b_in| {
            Box::pin(do_something(b_in))
        })))
        .await?;
        let tap = b.get_tap().await?;
        b.emit_msg(MessageA::One).await?;
        // Ignore the MessageA::One
        let _ = tap.0.recv();
        assert_eq!(MessageA::Two, tap.0.recv()?);
        Ok(())
    }

    async fn test_translator_cb(msg: MessageA) -> bool {
        matches!(msg, MessageA::One)
    }

    #[tokio::test]
    async fn test_translator_callback() -> Result<(), Box<dyn std::error::Error>> {
        let mut b = Broker::new();
        b.add_subsystem(Subsystem::TranslatorCallback(Box::new(|_, b_in| {
            Box::pin(test_translator_cb(b_in))
        })))
        .await?;
        let tap = b.get_tap().await?;
        b.emit_msg(MessageA::One).await?;
        assert_eq!(MessageA::One, tap.0.recv()?);
        b.emit_msg(MessageA::Two).await?;
        assert!(tap.0.try_recv().is_err());
        Ok(())
    }
}
