//! # Broker is an actor implementation for wasm and libc
//!
//! Using the `Broker` structure, it is possible to link several modules
//! which exchange messages and rely themselves on asynchronous messages.
//! Handling asynchronous messages between different modules is not easy,
//! as you need to know which modules need to be notified in case a new
//! message comes in.
//! If you want to have modularity, and the possibility to add and remove
//! modules, an actor base system comes in handy.
//!
//! # Subsystems
//!
//! Every Broker has a certain number of subsystems.
//! A handler is a step in the passage of a message through the Broker.
//! The following subsystems exist:
//!
//!
//!
//! # Example
//!
//! In the simplest example, a `Broker` can be used as a channel:
//!
//! ```rust
//! fn start_broker(){}
//! ```
//!
//! But to use it's full potential, a `Broker` has to 'consume' a structure
//! by adding it as a `Handler`:
//!
//! ```rust
//! fn start_broker(){}
//! ```

use core::fmt;
use std::{
    collections::HashMap,
    fmt::Formatter,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc,
    },
};

use futures::lock::Mutex;
use thiserror::Error;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::{nodeids::U256, tasks::spawn_local};
use flmacro::{platform_async_trait, target_send};

#[cfg(target_family = "wasm")]
pub mod asy {
    pub trait Async {}
    impl<T> Async for T {}
}
#[cfg(target_family = "unix")]
pub mod asy {
    pub trait Async: Sync + Send {}
    impl<T: Sync + Send> Async for T {}
}
use asy::*;

#[derive(Debug, Error)]
/// The only error that can happen is that sending to another broker fails.
/// This is mostly due to the fact that the other broker doesn't exist anymore.
pub enum BrokerError {
    #[error("While sending to {0}")]
    /// Couldn't send to this queue.
    SendQueue(String),
    #[error("Translation error")]
    Translation,
}

/// Identifies a broker for loop detection.
pub type BrokerID = U256;

/// The Destination of the message, and also handles forwarded messages
/// to make sure no loop is created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// To all handlers in this Broker
    All,
    /// Only to handlers which represent no tap - useful for tests
    NoTap,
    /// A forwarded message, with the IDs of the brokers which already handled that message
    Forwarded(Vec<BrokerID>),
    /// Tags a message as coming from this handler, to avoid putting it back into the same handler
    Handled(usize),
}

enum SubsystemAction<I, O> {
    Add(usize, Subsystem<I, O>),
    Remove(usize),
}

impl<I, O> fmt::Debug for SubsystemAction<I, O> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SubsystemAction::Add(id, ss) => write!(f, "Add({}, {ss:?})", id),
            SubsystemAction::Remove(id) => write!(f, "Remove({})", id),
        }
    }
}

pub trait Message: Async + Clone + fmt::Debug {}
impl<M: Async + Clone + fmt::Debug> Message for M {}

/// The Broker connects the different subsystem together and offers
/// a pub/sub system.
/// Every subsystem can subscribe to any number of messages.
/// Every subsystem can emit any number of messages.
/// Incoming messages are queued in a channel and are treated when
/// the `process` method is called.
pub struct Broker<I: Message + 'static, O: Message + 'static> {
    intern_tx: UnboundedSender<InternMessage<I, O>>,
    // Intern is in an always-locked state to make sure it's dropped last.
    intern: Arc<Mutex<Intern<I, O>>>,
    subsystems: Arc<Mutex<usize>>,
    id: BrokerID,
}

impl<I: 'static + Message, O: 'static + Message> std::fmt::Debug for Broker<I, O> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Broker")
            .field("intern_tx", &self.intern_tx)
            .field("subsystems", &self.subsystems)
            .field("id", &self.id)
            .finish()
    }
}

impl<I: 'static + Message, O: 'static + Message> Default for Broker<I, O> {
    /// Create a new Broker.
    fn default() -> Self {
        Self::new()
    }
}

impl<I: Message, O: Message> Clone for Broker<I, O> {
    /// Clone the Broker. The new Broker will communicate with the same "Intern" structure
    /// and share all messages. However, each Broker clone will have its own tap messages.
    fn clone(&self) -> Self {
        // log::warn!("{}/{}: Sending clone", self.id, self.type_str());
        self.intern_tx
            .send(InternMessage::Cloned)
            .expect("Couldn't inform broker::Intern of cloning");
        Self {
            intern_tx: self.intern_tx.clone(),
            intern: self.intern.clone(),
            subsystems: Arc::clone(&self.subsystems),
            id: self.id,
        }
    }
}

impl<I: Message + 'static, O: Message + 'static> Drop for Broker<I, O> {
    fn drop(&mut self) {
        // log::warn!("{}/{}: Sending Dropped", self.id, self.type_str());
        self.intern_tx
            .send(InternMessage::Dropped)
            .err()
            .map(|e| log::warn!("{}: Couldn't send 'Dropped': {e:?}", self.type_str()));
    }
}

impl<I: 'static + Message, O: 'static + Message> Broker<I, O> {
    /// Creates a new Broker of type <T> and initializes it without any subsystems.
    pub fn new() -> Self {
        let id = BrokerID::rnd();
        let (intern_tx, intern) = Intern::new(id);
        Self {
            intern_tx,
            intern,
            subsystems: Arc::new(Mutex::new(0)),
            id,
        }
    }

    /// Adds a new subsystem to send and/or receive messages.
    pub async fn add_subsystem(&mut self, ss: Subsystem<I, O>) -> anyhow::Result<usize> {
        let subsystem = {
            let mut subsystems = self.subsystems.lock().await;
            *subsystems += 1;
            *subsystems
        };
        self.intern_tx
            .send(InternMessage::Subsystem(SubsystemAction::Add(
                subsystem, ss,
            )))
            .map_err(|_| BrokerError::SendQueue("add_subsystem".into()))?;
        Ok(subsystem)
    }

    pub async fn add_translator_link<TI: Message + 'static, TO: Message + 'static>(
        &mut self,
        mut broker: Broker<TI, TO>,
        tr_o_ti: Translate<O, TI>,
        tr_to_i: Translate<TO, I>,
    ) -> anyhow::Result<usize> {
        broker.add_translator_o_ti(self.clone(), tr_to_i).await?;
        // The following panics
        self.add_translator_o_ti(broker, tr_o_ti).await
    }

    pub async fn add_translator_direct<TI: Message + 'static, TO: Message + 'static>(
        &mut self,
        mut broker: Broker<TI, TO>,
        tr_o_to: Translate<O, TO>,
        tr_ti_i: Translate<TI, I>,
    ) -> anyhow::Result<usize> {
        broker.add_translator_i_ti(self.clone(), tr_ti_i).await?;
        self.add_translator_o_to(broker, tr_o_to).await
    }

    pub async fn add_translator_i_ti<TI: Message + 'static, TO: Message + 'static>(
        &mut self,
        broker: Broker<TI, TO>,
        i_ti: Translate<I, TI>,
    ) -> anyhow::Result<usize> {
        self.add_translator(Box::new(Translator {
            broker,
            translate_fn_i_ti: Some(i_ti),
            translate_fn_i_to: None,
            translate_fn_o_ti: None,
            translate_fn_o_to: None,
        }))
        .await
    }

    pub async fn add_translator_i_to<TI: Message + 'static, TO: Message + 'static>(
        &mut self,
        broker: Broker<TI, TO>,
        i_to: Translate<I, TO>,
    ) -> anyhow::Result<usize> {
        self.add_translator(Box::new(Translator {
            broker,
            translate_fn_i_ti: None,
            translate_fn_i_to: Some(i_to),
            translate_fn_o_ti: None,
            translate_fn_o_to: None,
        }))
        .await
    }

    pub async fn add_translator_o_ti<TI: Message + 'static, TO: Message + 'static>(
        &mut self,
        broker: Broker<TI, TO>,
        o_ti: Translate<O, TI>,
    ) -> anyhow::Result<usize> {
        self.add_translator(Box::new(Translator {
            broker,
            translate_fn_i_ti: None,
            translate_fn_i_to: None,
            translate_fn_o_ti: Some(o_ti),
            translate_fn_o_to: None,
        }))
        .await
    }

    pub async fn add_translator_o_to<TI: Message + 'static, TO: Message + 'static>(
        &mut self,
        broker: Broker<TI, TO>,
        o_to: Translate<O, TO>,
    ) -> anyhow::Result<usize> {
        self.add_translator(Box::new(Translator {
            broker,
            translate_fn_i_ti: None,
            translate_fn_i_to: None,
            translate_fn_o_ti: None,
            translate_fn_o_to: Some(o_to),
        }))
        .await
    }

    /// Removes a subsystem from the list that will be applied to new messages.
    pub async fn remove_subsystem(&mut self, ss: usize) -> anyhow::Result<()> {
        self.intern_tx
            .send(InternMessage::Subsystem(SubsystemAction::Remove(ss)))
            .map_err(|_| BrokerError::SendQueue("remove_subsystem".into()))?;
        self.settle(vec![]).await
    }

    /// Adds an async tap to the subsystems that can be used to listen to messages.
    /// The async tap is returned.
    pub async fn get_tap_out(&mut self) -> anyhow::Result<(UnboundedReceiver<O>, usize)> {
        let (tx, rx) = unbounded_channel();
        let pos = self.add_subsystem(Subsystem::TapOut(tx)).await?;
        Ok((rx, pos))
    }

    /// Adds a synchronous tap that can be used to listen to messages.
    /// Care must be taken that the async handling will still continue while
    /// waiting for a message!
    /// For this reason it is better to use the `get_tap` method.
    /// Returns the synchronous tap.
    pub async fn get_tap_out_sync(&mut self) -> anyhow::Result<(Receiver<O>, usize)> {
        let (tx, rx) = channel();
        let pos = self.add_subsystem(Subsystem::TapSyncOut(tx)).await?;
        Ok((rx, pos))
    }

    /// Adds an async tap to the subsystems that can be used to listen to messages.
    /// The async tap is returned.
    pub async fn get_tap_in(&mut self) -> anyhow::Result<(UnboundedReceiver<I>, usize)> {
        let (tx, rx) = unbounded_channel();
        let pos = self.add_subsystem(Subsystem::TapIn(tx)).await?;
        Ok((rx, pos))
    }

    /// Adds a synchronous tap that can be used to listen to messages.
    /// Care must be taken that the async handling will still continue while
    /// waiting for a message!
    /// For this reason it is better to use the `get_tap` method.
    /// Returns the synchronous tap.
    pub async fn get_tap_in_sync(&mut self) -> anyhow::Result<(Receiver<I>, usize)> {
        let (tx, rx) = channel();
        let pos = self.add_subsystem(Subsystem::TapSyncIn(tx)).await?;
        Ok((rx, pos))
    }

    /// Emit a message to a given destination of other listeners.
    /// The message will be processed asynchronously.
    pub fn emit_msg_in_dest(&mut self, dst: Destination, msg: I) -> anyhow::Result<()> {
        Ok(self
            .intern_tx
            .send(InternMessage::MessageIn(dst, msg))
            .map_err(|_| BrokerError::SendQueue("emit_msg_dest".into()))?)
    }

    /// Emit a message to other listeners.
    /// The message will be processed asynchronously.
    pub fn emit_msg_in(&mut self, msg: I) -> anyhow::Result<()> {
        self.emit_msg_in_dest(Destination::All, msg)
    }

    /// Emit a message to a given destination of other listeners.
    /// The message will be processed asynchronously.
    pub fn emit_msg_out_dest(&mut self, dst: Destination, msg: O) -> anyhow::Result<()> {
        Ok(self
            .intern_tx
            .send(InternMessage::MessageOut(dst, msg))
            .map_err(|_| BrokerError::SendQueue("emit_msg_dest".into()))?)
    }

    /// Emit a message to other listeners.
    /// The message will be processed asynchronously.
    pub fn emit_msg_out(&mut self, msg: O) -> anyhow::Result<()> {
        self.emit_msg_out_dest(Destination::All, msg)
    }

    /// Emit a message to a given destination of other listeners and wait for all involved
    /// brokers to settle.
    pub async fn settle_msg_in_dest(&mut self, dst: Destination, msg: I) -> anyhow::Result<()> {
        self.intern_tx
            .send(InternMessage::MessageIn(dst, msg))
            .map_err(|_| BrokerError::SendQueue("settle_msg_dest".into()))?;
        self.settle(vec![]).await
    }

    /// Emit a message to other listeners and wait for all involved brokers to settle.
    pub async fn settle_msg_in(&mut self, msg: I) -> anyhow::Result<()> {
        self.settle_msg_in_dest(Destination::All, msg).await
    }

    /// Emit a message to a given destination of other listeners and wait for all involved
    /// brokers to settle.
    pub async fn settle_msg_out_dest(&mut self, dst: Destination, msg: O) -> anyhow::Result<()> {
        self.intern_tx
            .send(InternMessage::MessageOut(dst, msg))
            .map_err(|_| BrokerError::SendQueue("settle_msg_dest".into()))?;
        self.settle(vec![]).await
    }

    /// Emit a message to other listeners and wait for all involved brokers to settle.
    pub async fn settle_msg_out(&mut self, msg: O) -> anyhow::Result<()> {
        self.settle_msg_out_dest(Destination::All, msg).await
    }

    /// Connects to another Broker.
    /// The translations are defined using TranslateFrom and TranslateInto.
    /// If you only own the types TI and TO, then you can call link_bi on
    /// the 'other' Broker.
    pub async fn link_bi<TI: 'static + Message, TO: 'static + Message>(
        &mut self,
        other: Broker<TI, TO>,
    ) -> anyhow::Result<()>
    where
        I: TranslateFrom<TO>,
        O: TranslateInto<TI>,
    {
        self.add_translator_link(other, Box::new(O::translate), Box::new(I::translate))
            .await?;
        Ok(())
    }

    /// Connects to another Broker, but in a direct way:
    /// I forwards to TI, O forwards to TO.
    pub async fn link_direct<TI: 'static + Message, TO: 'static + Message>(
        &mut self,
        other: Broker<TI, TO>,
    ) -> anyhow::Result<()>
    where
        I: TranslateFrom<TI>,
        O: TranslateInto<TO>,
    {
        self.add_translator_direct(other, Box::new(O::translate), Box::new(I::translate))
            .await?;
        Ok(())
    }

    /// Waits for all messages in the queue to be forwarded / handled, before returning.
    /// It also calls all brokers that are signed up as forwarding targets.
    /// The caller argument is to be used when recursively settling, to avoid
    /// endless loops.
    pub async fn settle(&mut self, callers: Vec<BrokerID>) -> anyhow::Result<()> {
        let (tx, mut rx) = unbounded_channel();
        self.intern_tx
            .send(InternMessage::Settle(callers.clone(), tx))
            .map_err(|_| BrokerError::SendQueue("settle".into()))?;
        rx.recv().await;
        Ok(())
    }

    fn type_str(&self) -> String {
        format!(
            "<{},{}>",
            std::any::type_name::<I>(),
            std::any::type_name::<O>()
        )
    }
}

impl<I: 'static + Message, O: 'static + Message> Broker<I, O> {
    pub async fn add_handler(
        &mut self,
        handler: SubsystemHandlerBox<I, O>,
    ) -> anyhow::Result<usize> {
        self.add_subsystem(Subsystem::Handler(handler)).await
    }

    pub async fn add_translator(
        &mut self,
        translate: SubsystemTranslatorBox<I, O>,
    ) -> anyhow::Result<usize> {
        self.add_subsystem(Subsystem::Translator(translate)).await
    }
}

enum InternMessage<I: Message, O: Message> {
    Subsystem(SubsystemAction<I, O>),
    MessageIn(Destination, I),
    MessageOut(Destination, O),
    Settle(Vec<BrokerID>, UnboundedSender<bool>),
    Cloned,
    Dropped,
}

struct Intern<I: Message + 'static, O: Message + 'static> {
    main_rx: UnboundedReceiver<InternMessage<I, O>>,
    subsystems: HashMap<usize, Subsystem<I, O>>,
    msg_queue_in: Vec<(Destination, I)>,
    msg_queue_out: Vec<(Destination, O)>,
    id: BrokerID,
    copies: usize,
}

impl<I: Message + 'static, O: Message + 'static> Intern<I, O> {
    fn new(
        id: BrokerID,
    ) -> (
        UnboundedSender<InternMessage<I, O>>,
        Arc<Mutex<Intern<I, O>>>,
    ) {
        log::trace!(
            "Creating Broker {} for {}->{}",
            id,
            std::any::type_name::<I>(),
            std::any::type_name::<O>()
        );
        let (main_tx, main_rx) = unbounded_channel::<InternMessage<I, O>>();
        let intern = Arc::new(Mutex::new(Self {
            main_rx,
            subsystems: HashMap::new(),
            msg_queue_in: vec![],
            msg_queue_out: vec![],
            id,
            copies: 1,
        }));

        let intern_arc = intern.clone();
        spawn_local(async move {
            let mut intern = intern_arc.try_lock().expect("Getting global lock");
            loop {
                if !intern.get_msg().await {
                    // log::warn!(
                    //     "{}: Closed Intern_locked.main_rx for {}",
                    //     intern.id,
                    //     intern.type_str()
                    // );
                    return;
                }

                if !intern.msg_queue_in.is_empty() || !intern.msg_queue_out.is_empty() {
                    match intern.process().await {
                        Ok(nbr) => log::trace!("{}: Processed {nbr} messages", intern.type_str()),
                        Err(e) => {
                            log::error!("{}: Couldn't process: {e:?}", intern.type_str());
                        }
                    }
                }
            }
        });

        (main_tx, intern)
    }

    // The process method goes through all incoming messages and treats them from the first
    // subscribed subsystem to the last.
    // This means that it's possible if a subscription and a message are pending at the same
    // time, that you won't get what you expect.
    async fn process(&mut self) -> anyhow::Result<usize> {
        let mut msg_count: usize = 0;
        loop {
            msg_count += self.msg_queue_in.len() + self.msg_queue_out.len();
            self.process_once().await?;
            if self.msg_queue_in.is_empty() && self.msg_queue_out.is_empty() {
                break;
            }
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
                log::warn!(
                    "{}/{} Closed queue before Close arrived",
                    self.id,
                    self.type_str()
                );
                return false;
            }
        };
        match msg_queue {
            InternMessage::Subsystem(ss) => {
                log::trace!("{self:p}/{} subsystem action {ss:?}", self.type_str());
                self.subsystem_action(ss);
                return true;
            }
            InternMessage::MessageIn(dst, msg) => self.msg_queue_in.push((dst, msg)),
            InternMessage::MessageOut(dst, msg) => self.msg_queue_out.push((dst, msg)),
            InternMessage::Settle(list, reply) => {
                let type_str = self.type_str();
                if !list.contains(&self.id) {
                    let mut list = list.clone();
                    list.push(self.id);
                    for (_, ss) in self.subsystems.iter_mut() {
                        ss.settle(list.clone())
                            .await
                            .err()
                            .map(|e| log::error!("{}: While settling: {e:?}", type_str));
                    }
                }
                reply
                    .send(true)
                    .err()
                    .map(|e| log::error!("{}: Couldn't send: {e:?}", type_str));
                return true;
            }
            InternMessage::Dropped => {
                self.copies -= 1;
                // log::warn!(
                //     "{}/{}: Copies now: {}",
                //     self.id,
                //     self.type_str(),
                //     self.copies
                // );
                if self.copies == 0 {
                    self.subsystems = HashMap::new();
                    return false;
                } else {
                    return true;
                }
            }
            InternMessage::Cloned => {
                self.copies += 1;
                // log::warn!(
                //     "{}/{}: Copies now: {}",
                //     self.id,
                //     self.type_str(),
                //     self.copies
                // );
            }
        };

        true
    }

    /// Adds a SubsystemInit
    fn subsystem_action(&mut self, ssa: SubsystemAction<I, O>) {
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
    // 1. Translate all messages, and remove the translated ones
    // 2. Send all messages to the taps, except those with Destination::NoTap
    // 3. Send all messages to the handlers, and collect messages
    //
    // Every tap returning an error is considered closed and removed from
    // the list of subsystems.
    async fn process_once(&mut self) -> anyhow::Result<()> {
        self.process_translate_out_messages().await;
        self.process_translate_in_messages().await;

        let mut subsystems_error = self.send_tap_out().await;
        subsystems_error.extend(self.send_tap_in().await);

        self.msg_queue_out.clear();

        subsystems_error.extend(self.process_handle_messages().await);

        self.msg_queue_in.clear();

        for index in subsystems_error.iter().rev() {
            self.subsystem_action(SubsystemAction::Remove(*index));
        }

        Ok(())
    }

    // First run translators on the messages.
    async fn process_translate_out_messages(&mut self) {
        for (dst, msg) in &self.msg_queue_out {
            let mut trail = vec![];
            if let Destination::Forwarded(t) = dst.clone() {
                trail.extend(t);
            }
            if trail.contains(&self.id) {
                log::warn!(
                    "{}: Endless forward-loop detected, aborting",
                    self.type_str()
                );
                continue;
            }
            trail.push(self.id);

            for (_, ss) in self
                .subsystems
                .iter_mut()
                .filter(|(_, ss)| ss.is_translator())
            {
                match ss {
                    Subsystem::Translator(ref mut translator) => {
                        translator
                            .translate_output(trail.clone(), msg.clone())
                            .await;
                    }
                    _ => (),
                }
            }
        }
    }

    // First run translators on the messages.
    async fn process_translate_in_messages(&mut self) {
        for (dst, msg) in &self.msg_queue_in {
            let mut trail = vec![];
            if let Destination::Forwarded(t) = dst.clone() {
                trail.extend(t);
            }
            if trail.contains(&self.id) {
                log::warn!(
                    "{}: Endless forward-loop detected, aborting",
                    self.type_str()
                );
                continue;
            }
            trail.push(self.id);

            for (_, ss) in self
                .subsystems
                .iter_mut()
                .filter(|(_, ss)| ss.is_translator())
            {
                match ss {
                    Subsystem::Translator(ref mut translator) => {
                        translator.translate_input(trail.clone(), msg.clone()).await;
                    }
                    _ => (),
                }
            }
        }
    }

    // Then send the messages to the taps, except Destination::NoTap.
    async fn send_tap_out(&mut self) -> Vec<usize> {
        let mut faulty = vec![];
        let msgs: Vec<O> = self
            .msg_queue_out
            .iter()
            .filter(|(dst, _)| dst != &Destination::NoTap)
            .map(|(_, msg)| msg.clone())
            .collect();

        let type_str = self.type_str();
        for (i, ss) in self.subsystems.iter_mut() {
            if let Err(e) = ss.send_tap_out(msgs.clone()).await {
                log::trace!(
                    "{}: Couldn't send {msgs:?} to tap-out: {e:?}::{} - perhaps you should use 'remove_subsystem'?",
                    type_str,
                    i
                );
                faulty.push(*i);
            }
        }

        faulty
    }

    // Then send the messages to the taps, except Destination::NoTap.
    async fn send_tap_in(&mut self) -> Vec<usize> {
        let mut faulty = vec![];
        let msgs: Vec<I> = self
            .msg_queue_in
            .iter()
            .filter(|(dst, _)| dst != &Destination::NoTap)
            .map(|(_, msg)| msg.clone())
            .collect();

        let type_str = self.type_str();
        for (i, ss) in self.subsystems.iter_mut() {
            if let Err(e) = ss.send_tap_in(msgs.clone()).await {
                log::trace!(
                    "{}: Couldn't send to tap-in: {e:?} - perhaps you should use 'remove_subsystem'?",
                    type_str
                );
                faulty.push(*i);
            }
        }

        faulty
    }

    // Finally send messages to all other subsystems, and collect
    // new messages for next call to 'process'.
    async fn process_handle_messages(&mut self) -> Vec<usize> {
        if self.msg_queue_in.is_empty() {
            return vec![];
        }
        let mut ss_remove = vec![];
        let mut new_msg_queue = vec![];
        let type_str = self.type_str();
        for (index_ss, ss) in self.subsystems.iter_mut().filter(|(_, ss)| ss.is_handler()) {
            let msgs: Vec<I> = self
                .msg_queue_in
                .iter()
                .filter(|nm| match nm.0 {
                    Destination::Handled(i) => index_ss != &i,
                    _ => true,
                })
                .map(|nm| &nm.1)
                .cloned()
                .collect();
            match ss.send_handler(*index_ss, msgs.clone()).await {
                Ok(mut new_msgs) => {
                    new_msg_queue.append(&mut new_msgs);
                }
                Err(e) => {
                    ss_remove.push(*index_ss);
                    log::error!("{}: While sending messages: {e}", type_str);
                }
            }
        }
        self.msg_queue_out.append(&mut new_msg_queue);

        ss_remove
    }

    fn type_str(&self) -> String {
        format!(
            "<{},{}>",
            std::any::type_name::<I>(),
            std::any::type_name::<O>()
        )
    }
}

impl<I: Message + 'static, O: Message + 'static> Drop for Intern<I, O> {
    fn drop(&mut self) {
        // log::warn!(
        //     "{}/{}: Intern dropped at counter {}",
        //     self.id,
        //     self.type_str(),
        //     self.copies
        // );
    }
}

/// Subsystems available in a Broker.
/// Every subsystem can be added zero, one, or more times.
pub enum Subsystem<I, O> {
    TapIn(UnboundedSender<I>),
    TapOut(UnboundedSender<O>),
    TapSyncIn(Sender<I>),
    TapSyncOut(Sender<O>),
    Handler(SubsystemHandlerBox<I, O>),
    Translator(SubsystemTranslatorBox<I, O>),
}

impl<I: Message, O: Message> Subsystem<I, O> {
    async fn send_tap_out(&mut self, msgs: Vec<O>) -> anyhow::Result<()> {
        Ok(match self {
            Self::TapSyncOut(s) => {
                for msg in msgs {
                    s.send(msg.clone())
                        .map_err(|_| BrokerError::SendQueue("send_tap".into()))?;
                }
            }
            Self::TapOut(s) => {
                for msg in msgs {
                    s.send(msg.clone())
                        .map_err(|_| BrokerError::SendQueue("send_tap_async".into()))?;
                }
            }
            _ => (),
        })
    }

    async fn send_tap_in(&mut self, msgs: Vec<I>) -> anyhow::Result<()> {
        Ok(match self {
            Self::TapSyncIn(s) => {
                for msg in msgs {
                    s.send(msg.clone())
                        .map_err(|_| BrokerError::SendQueue("send_tap".into()))?;
                }
            }
            Self::TapIn(s) => {
                for msg in msgs {
                    s.send(msg.clone())
                        .map_err(|_| BrokerError::SendQueue("send_tap_async".into()))?;
                }
            }
            _ => (),
        })
    }

    async fn send_handler(
        &mut self,
        index: usize,
        msgs: Vec<I>,
    ) -> anyhow::Result<Vec<(Destination, O)>> {
        Ok(match self {
            Self::Handler(h) => {
                let ret = h.messages(msgs).await;
                ret.into_iter()
                    .map(|m| (Destination::Handled(index), m))
                    .collect()
            }
            _ => vec![],
        })
    }

    fn is_translator(&self) -> bool {
        matches!(self, Self::Translator(_))
    }

    fn is_handler(&self) -> bool {
        matches!(self, Self::Handler(_))
    }

    async fn settle(&mut self, callers: Vec<BrokerID>) -> anyhow::Result<()> {
        match self {
            Subsystem::Translator(tr) => tr.settle(callers).await,
            _ => Ok(()),
        }
    }
}

impl<I, O> fmt::Debug for Subsystem<I, O> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::TapSyncIn(_) => write!(f, "TapSyncIn"),
            Self::TapSyncOut(_) => write!(f, "TapSyncOut"),
            Self::TapIn(_) => write!(f, "TapAsyncIn"),
            Self::TapOut(_) => write!(f, "TapAsyncOut"),
            Self::Handler(_) => write!(f, "Handler"),
            Self::Translator(_) => write!(f, "Translator"),
        }
    }
}

#[platform_async_trait()]
#[target_send()]
pub trait SubsystemHandler<I: Async, O: Async> {
    async fn messages(&mut self, from_broker: Vec<I>) -> Vec<O>;
}

#[platform_async_trait()]
#[target_send()]
pub trait SubsystemTranslator<I: Async, O: Async> {
    async fn translate_input(&mut self, trail: Vec<BrokerID>, from_broker: I);
    async fn translate_output(&mut self, trail: Vec<BrokerID>, from_broker: O);
    async fn settle(&mut self, callers: Vec<BrokerID>) -> anyhow::Result<()>;
}

#[target_send()]
pub type Translate<A, B> = Box<dyn Fn(A) -> Option<B> + 'static>;

pub struct Translator<I: Message, O: Message, TI: Message + 'static, TO: Message + 'static> {
    broker: Broker<TI, TO>,
    translate_fn_i_ti: Option<Translate<I, TI>>,
    translate_fn_i_to: Option<Translate<I, TO>>,
    translate_fn_o_ti: Option<Translate<O, TI>>,
    translate_fn_o_to: Option<Translate<O, TO>>,
}

impl<I: Message, O: Message, TI: Message + 'static, TO: Message + 'static>
    Translator<I, O, TI, TO>
{
    async fn emit_ti_to(&mut self, trail: Vec<BrokerID>, msg_ti: Option<TI>, msg_to: Option<TO>) {
        if let Some(msg_tr) = msg_ti {
            if let Err(e) = self
                .broker
                .emit_msg_in_dest(Destination::Forwarded(trail.clone()), msg_tr)
            {
                log::error!("Translated message couldn't be queued: {e}",);
            };
        }
        if let Some(msg_tr) = msg_to {
            if let Err(e) = self
                .broker
                .emit_msg_out_dest(Destination::Forwarded(trail), msg_tr)
            {
                log::error!("Translated message couldn't be queued: {e}",);
            };
        }
    }
}

#[platform_async_trait()]
impl<I: Message, O: Message, TI: Message + 'static, TO: Message + 'static> SubsystemTranslator<I, O>
    for Translator<I, O, TI, TO>
{
    async fn translate_input(&mut self, trail: Vec<BrokerID>, msg: I) {
        self.emit_ti_to(
            trail,
            self.translate_fn_i_ti
                .as_ref()
                .and_then(|tr| tr(msg.clone())),
            self.translate_fn_i_to.as_ref().and_then(|tr| tr(msg)),
        )
        .await;
    }

    async fn translate_output(&mut self, trail: Vec<BrokerID>, msg: O) {
        self.emit_ti_to(
            trail,
            self.translate_fn_o_ti
                .as_ref()
                .and_then(|tr| tr(msg.clone())),
            self.translate_fn_o_to.as_ref().and_then(|tr| tr(msg)),
        )
        .await;
    }

    async fn settle(&mut self, callers: Vec<BrokerID>) -> anyhow::Result<()> {
        if !callers.contains(&self.broker.id) {
            self.broker.settle(callers).await?;
        }
        Ok(())
    }
}

#[platform_async_trait()]
pub trait TranslateFrom<T: Message>: Sized {
    fn translate(msg: T) -> Option<Self>;
}

#[platform_async_trait()]
pub trait TranslateInto<T: Message> {
    fn translate(self) -> Option<T>;
}

#[platform_async_trait()]
pub trait TranslateLink<I: Message + 'static, O: Message + 'static, TI: Message, TO: Message>:
    Async + Clone
{
    fn translate_o_ti(&self, msg: O) -> Option<TI>;
    fn translate_to_i(&self, msg: TO) -> Option<I>;
}

#[platform_async_trait()]
pub trait TranslateDirect<I: Message, O: Message, TI: Message, TO: Message> {
    fn translate_o_ti(msg: O) -> Option<TO>;
    fn translate_to_i(msg: I) -> Option<TI>;
}

#[cfg(test)]
mod tests {
    use thiserror::Error;

    use crate::{start_logging, start_logging_filter_level};

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    pub enum BrokerTestIn {
        MsgA,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum BrokerTestOut {
        MsgB,
    }

    pub struct Tps {
        reply: Vec<(BrokerTestIn, BrokerTestOut)>,
    }

    #[platform_async_trait()]
    impl SubsystemHandler<BrokerTestIn, BrokerTestOut> for Tps {
        async fn messages(&mut self, msgs: Vec<BrokerTestIn>) -> Vec<BrokerTestOut> {
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

    /// Test the Broker with two subsystems.
    #[tokio::test]
    async fn test_broker_new() -> anyhow::Result<()> {
        start_logging();

        let bm_a = BrokerTestIn::MsgA;
        let bm_b = BrokerTestOut::MsgB;

        let broker = &mut Broker::new();
        // Add a first subsystem that will reply 'msg_b' when it
        // receives 'msg_a'.
        broker
            .add_subsystem(Subsystem::Handler(Box::new(Tps {
                reply: vec![(bm_a.clone(), bm_b.clone())],
            })))
            .await?;
        let (tap, _) = broker.get_tap_out_sync().await?;

        // Should reply to msg_a, so the tap should have 1 message - the original
        // and the reply.
        broker.settle_msg_in(bm_a.clone()).await?;
        assert_eq!(tap.try_iter().count(), 1);

        // Add the same subsystem, now it should get 2 messages - the two replies.
        broker
            .add_subsystem(Subsystem::Handler(Box::new(Tps {
                reply: vec![(bm_a.clone(), bm_b)],
            })))
            .await?;
        broker.settle_msg_in(bm_a).await?;
        assert_eq!(tap.try_iter().count(), 2);

        Ok(())
    }

    #[derive(Clone, PartialEq, Debug)]
    enum MessageAI {
        One,
        _Two,
        _Three,
    }

    #[derive(Clone, PartialEq, Debug)]
    enum MessageAO {
        _Un,
        Deux,
        Trois,
    }

    #[derive(Clone, PartialEq, Debug)]
    enum MessageBI {
        _One,
        Two,
        _Three,
    }

    #[derive(Clone, PartialEq, Debug)]
    enum MessageBO {
        Un,
        _Deux,
        Trois,
    }

    fn translate_ab(msg: MessageAO) -> Option<MessageBI> {
        match msg {
            MessageAO::Deux => Some(MessageBI::Two),
            _ => None,
        }
    }

    fn translate_ba(msg: MessageBO) -> Option<MessageAI> {
        match msg {
            MessageBO::Un => Some(MessageAI::One),
            _ => None,
        }
    }

    #[derive(Error, Debug)]
    enum ConvertError {
        #[error("Wrong conversion")]
        Conversion(String),
        #[error(transparent)]
        Broker(#[from] BrokerError),
    }

    #[tokio::test]
    async fn link() -> anyhow::Result<()> {
        start_logging_filter_level(vec![], log::LevelFilter::Info);

        let mut broker_a: Broker<MessageAI, MessageAO> = Broker::new();
        let (tap_a_in, _) = broker_a.get_tap_in_sync().await?;
        let mut broker_b: Broker<MessageBI, MessageBO> = Broker::new();
        let (tap_b_in, _) = broker_b.get_tap_in_sync().await?;

        broker_a
            .add_translator_link(
                broker_b.clone(),
                Box::new(translate_ab),
                Box::new(translate_ba),
            )
            .await?;

        broker_a.settle_msg_out(MessageAO::Deux).await?;
        if let Ok(msg) = tap_b_in.try_recv() {
            assert_eq!(MessageBI::Two, msg);
        } else {
            return Err(ConvertError::Conversion("A to B".to_string()).into());
        }

        broker_b.settle_msg_out(MessageBO::Un).await?;
        if let Ok(msg) = tap_a_in.try_recv() {
            assert_eq!(MessageAI::One, msg);
        } else {
            return Err(ConvertError::Conversion("B to A".to_string()).into());
        }

        broker_a.settle_msg_out(MessageAO::Trois).await?;
        // Remove the untranslated message
        assert!(tap_b_in.try_recv().is_err());
        broker_b.settle_msg_out(MessageBO::Trois).await?;
        // Remove the untranslated message
        assert!(tap_a_in.try_recv().is_err());

        Ok(())
    }

    async fn _do_something(from_broker: Vec<MessageAI>) -> Vec<(Destination, MessageAO)> {
        from_broker
            .iter()
            .filter(|msg| msg == &&MessageAI::One)
            .map(|_| (Destination::All, MessageAO::_Un))
            .collect()
    }

    #[derive(Clone, std::fmt::Debug, PartialEq)]
    enum MessageAIO {
        Input(MessageAI),
        _Output(MessageAO),
    }

    impl TranslateInto<MessageAIO> for MessageAO {
        fn translate(self) -> Option<MessageAIO> {
            Some(match self {
                MessageAO::_Un => MessageAIO::Input(MessageAI::One),
                MessageAO::Deux => MessageAIO::Input(MessageAI::_Two),
                MessageAO::Trois => MessageAIO::Input(MessageAI::_Three),
            })
        }
    }

    impl TranslateFrom<MessageAIO> for MessageAI {
        fn translate(value: MessageAIO) -> Option<MessageAI> {
            if let MessageAIO::_Output(out) = value {
                Some(match out {
                    MessageAO::_Un => MessageAI::One,
                    MessageAO::Deux => MessageAI::_Two,
                    MessageAO::Trois => MessageAI::_Three,
                })
            } else {
                None
            }
        }
    }

    // #[tokio::test]
    // async fn test_broker() -> anyhow::Result<()> {
    //     start_logging_filter_level(vec![], log::LevelFilter::Info);

    //     let mut bio = Broker::<MessageAI, MessageAO>::new();
    //     let mut b = broker::Broker::<MessageAIO>::new();
    //     bio.link_broker(b.clone()).await?;

    //     let (bio_in, _) = bio.get_tap_in_sync().await?;
    //     let (b_tap, _) = b.get_tap_sync().await?;

    //     bio.settle_msg_out(MessageAO::Un).await?;
    //     assert_eq!(
    //         MessageAIO::Input(MessageAI::One),
    //         b_tap.recv().expect("receive b_tap")
    //     );

    //     b.settle_msg(MessageAIO::Output(MessageAO::Deux)).await?;
    //     assert_eq!(MessageAI::_Two, bio_in.recv().expect("receive bio_out"));
    //     Ok(())
    // }
}
