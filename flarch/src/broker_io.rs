//! # BrokerIO is an actor implementation for wasm and libc
//!
//! Using the `BrokerIO` structure, it is possible to link several modules
//! which exchange messages and rely themselves on asynchronous messages.
//! Handling asynchronous messages between different modules is not easy,
//! as you need to know which modules need to be notified in case a new
//! message comes in.
//! If you want to have modularity, and the possibility to add and remove
//! modules, an actor base system comes in handy.
//!
//! # Subsystems
//!
//! Every brokerio has a certain number of subsystems.
//! A handler is a step in the passage of a message through the brokerio.
//! The following subsystems exist:
//!
//!
//!
//! # Example
//!
//! In the simplest example, a `BrokerIO` can be used as a channel:
//!
//! ```rust
//! fn start_broker(){}
//! ```
//!
//! But to use it's full potential, a `BrokerIO` has to 'consume' a structure
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

use futures::{future::BoxFuture, lock::Mutex};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::{broker::{self, asy::*}, tasks::spawn_local};
use flarch_macro::platform_async_trait;

pub type BrokerError = broker::BrokerError;

/// Identifies a brokerio for loop detection.
pub type BrokerID = broker::BrokerID;

/// The Destination of the message, and also handles forwarded messages
/// to make sure no loop is created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// To all handlers in this brokerio
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

/// The brokerio connects the different subsystem together and offers
/// a pub/sub system.
/// Every subsystem can subscribe to any number of messages.
/// Every subsystem can emit any number of messages.
/// Incoming messages are queued in a channel and are treated when
/// the `process` method is called.
pub struct BrokerIO<I: Message, O: Message> {
    intern_tx: UnboundedSender<InternMessage<I, O>>,
    subsystems: Arc<Mutex<usize>>,
    id: BrokerID,
}

impl<I: 'static + Message, O: 'static + Message> Default for BrokerIO<I, O> {
    /// Create a new brokerio.
    fn default() -> Self {
        Self::new()
    }
}

impl<I: Message, O: Message> Clone for BrokerIO<I, O> {
    /// Clone the brokerio. The new brokerio will communicate with the same "Intern" structure
    /// and share all messages. However, each brokerio clone will have its own tap messages.
    fn clone(&self) -> Self {
        Self {
            intern_tx: self.intern_tx.clone(),
            subsystems: Arc::clone(&self.subsystems),
            id: self.id,
        }
    }
}

impl<I: 'static + Message, O: 'static + Message> BrokerIO<I, O> {
    /// Creates a new brokerio of type <T> and initializes it without any subsystems.
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
    pub async fn add_subsystem(&mut self, ss: Subsystem<I, O>) -> Result<usize, BrokerError> {
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
        mut brokerio: BrokerIO<TI, TO>,
        tr_o_ti: Translate<O, TI>,
        tr_to_i: Translate<TO, I>,
    ) -> Result<usize, BrokerError> {
        brokerio
            .add_translator(Box::new(Translator {
                brokerio: self.clone(),
                translate_fn_i_ti: None,
                translate_fn_i_to: None,
                translate_fn_o_ti: Some(tr_to_i),
                translate_fn_o_to: None,
            }))
            .await?;
        self.add_translator(Box::new(Translator {
            brokerio,
            translate_fn_i_ti: None,
            translate_fn_i_to: None,
            translate_fn_o_ti: Some(tr_o_ti),
            translate_fn_o_to: None,
        }))
        .await
    }

    pub async fn add_translator_direct<TI: Message + 'static, TO: Message + 'static>(
        &mut self,
        mut brokerio: BrokerIO<TI, TO>,
        tr_ti_i: Translate<TI, I>,
        tr_o_to: Translate<O, TO>,
    ) -> Result<usize, BrokerError> {
        brokerio
            .add_translator(Box::new(Translator {
                brokerio: self.clone(),
                translate_fn_i_ti: Some(tr_ti_i),
                translate_fn_i_to: None,
                translate_fn_o_ti: None,
                translate_fn_o_to: None,
            }))
            .await?;
        self.add_translator(Box::new(Translator {
            brokerio,
            translate_fn_i_ti: None,
            translate_fn_i_to: None,
            translate_fn_o_ti: None,
            translate_fn_o_to: Some(tr_o_to),
        }))
        .await
    }

    /// Removes a subsystem from the list that will be applied to new messages.
    pub async fn remove_subsystem(&mut self, ss: usize) -> Result<(), BrokerError> {
        self.intern_tx
            .send(InternMessage::Subsystem(SubsystemAction::Remove(ss)))
            .map_err(|_| BrokerError::SendQueue("remove_subsystem".into()))?;
        self.settle(vec![]).await
    }

    /// Adds an async tap to the subsystems that can be used to listen to messages.
    /// The async tap is returned.
    pub async fn get_tap_out(&mut self) -> Result<(UnboundedReceiver<O>, usize), BrokerError> {
        let (tx, rx) = unbounded_channel();
        let pos = self.add_subsystem(Subsystem::TapOut(tx)).await?;
        Ok((rx, pos))
    }

    /// Adds a synchronous tap that can be used to listen to messages.
    /// Care must be taken that the async handling will still continue while
    /// waiting for a message!
    /// For this reason it is better to use the `get_tap` method.
    /// Returns the synchronous tap.
    pub async fn get_tap_out_sync(&mut self) -> Result<(Receiver<O>, usize), BrokerError> {
        let (tx, rx) = channel();
        let pos = self.add_subsystem(Subsystem::TapSyncOut(tx)).await?;
        Ok((rx, pos))
    }

    /// Adds an async tap to the subsystems that can be used to listen to messages.
    /// The async tap is returned.
    pub async fn get_tap_in(&mut self) -> Result<(UnboundedReceiver<I>, usize), BrokerError> {
        let (tx, rx) = unbounded_channel();
        let pos = self.add_subsystem(Subsystem::TapIn(tx)).await?;
        Ok((rx, pos))
    }

    /// Adds a synchronous tap that can be used to listen to messages.
    /// Care must be taken that the async handling will still continue while
    /// waiting for a message!
    /// For this reason it is better to use the `get_tap` method.
    /// Returns the synchronous tap.
    pub async fn get_tap_in_sync(&mut self) -> Result<(Receiver<I>, usize), BrokerError> {
        let (tx, rx) = channel();
        let pos = self.add_subsystem(Subsystem::TapSyncIn(tx)).await?;
        Ok((rx, pos))
    }

    /// Emit a message to a given destination of other listeners.
    /// The message will be processed asynchronously.
    pub fn emit_msg_in_dest(&mut self, dst: Destination, msg: I) -> Result<(), BrokerError> {
        self.intern_tx
            .send(InternMessage::MessageIn(dst, msg))
            .map_err(|_| BrokerError::SendQueue("emit_msg_dest".into()))
    }

    /// Emit a message to other listeners.
    /// The message will be processed asynchronously.
    pub fn emit_msg_in(&mut self, msg: I) -> Result<(), BrokerError> {
        self.emit_msg_in_dest(Destination::All, msg)
    }

    /// Emit a message to a given destination of other listeners.
    /// The message will be processed asynchronously.
    pub fn emit_msg_out_dest(&mut self, dst: Destination, msg: O) -> Result<(), BrokerError> {
        self.intern_tx
            .send(InternMessage::MessageOut(dst, msg))
            .map_err(|_| BrokerError::SendQueue("emit_msg_dest".into()))
    }

    /// Emit a message to other listeners.
    /// The message will be processed asynchronously.
    pub fn emit_msg_out(&mut self, msg: O) -> Result<(), BrokerError> {
        self.emit_msg_out_dest(Destination::All, msg)
    }

    /// Emit a message to a given destination of other listeners and wait for all involved
    /// brokers to settle.
    pub async fn settle_msg_in_dest(
        &mut self,
        dst: Destination,
        msg: I,
    ) -> Result<(), BrokerError> {
        self.intern_tx
            .send(InternMessage::MessageIn(dst, msg))
            .map_err(|_| BrokerError::SendQueue("settle_msg_dest".into()))?;
        self.settle(vec![]).await
    }

    /// Emit a message to other listeners and wait for all involved brokers to settle.
    pub async fn settle_msg_in(&mut self, msg: I) -> Result<(), BrokerError> {
        self.settle_msg_in_dest(Destination::All, msg).await
    }

    /// Emit a message to a given destination of other listeners and wait for all involved
    /// brokers to settle.
    pub async fn settle_msg_out_dest(
        &mut self,
        dst: Destination,
        msg: O,
    ) -> Result<(), BrokerError> {
        self.intern_tx
            .send(InternMessage::MessageOut(dst, msg))
            .map_err(|_| BrokerError::SendQueue("settle_msg_dest".into()))?;
        self.settle(vec![]).await
    }

    /// Emit a message to other listeners and wait for all involved brokers to settle.
    pub async fn settle_msg_out(&mut self, msg: O) -> Result<(), BrokerError> {
        self.settle_msg_out_dest(Destination::All, msg).await
    }

    /// Connects to another brokerio.
    /// The translations are defined using TranslateFrom and TranslateInto.
    /// If you don't own the types I and O, then you can call link_bi on
    /// the 'other' brokerio.
    pub async fn link_bi<TI: 'static + Message, TO: 'static + Message>(
        &mut self,
        other: BrokerIO<TI, TO>,
    ) -> Result<(), BrokerError>
    where
        I: TranslateFrom<TO>,
        O: TranslateInto<TI>,
    {
        self.add_translator_link(other, Box::new(O::translate), Box::new(I::translate))
            .await?;
        Ok(())
    }

    // /// Connects to another brokerio.
    // /// The translations are defined using TranslateFrom and TranslateInto.
    // /// If you don't own the types I and O, then you can call link_bi on
    // /// the 'other' brokerio.
    // pub async fn link_bi_dyn<TI: 'static + Message, TO: 'static + Message>(
    //     &mut self,
    //     other: BrokerIO<TI, TO>,
    //     translate: Box<dyn TranslateLink<I, O, TI, TO>>,
    // ) -> Result<(), BrokerError> {
    //     self.add_translator_link(
    //         other,
    //         Box::new(|msg| translate.clone().translate_o_ti(msg)),
    //         Box::new(|msg| translate.translate_to_i(msg)),
    //     )
    //     .await?;
    //     Ok(())
    // }

    /// Connects to another brokerio, but in a direct way:
    /// I forwards to TI, O forwards to TO.
    pub async fn link_direct<TI: 'static + Message, TO: 'static + Message>(
        &mut self,
        other: BrokerIO<TI, TO>,
    ) -> Result<(), BrokerError>
    where
        I: TranslateFrom<TI>,
        O: TranslateInto<TO>,
    {
        self.add_translator_direct(other, Box::new(I::translate), Box::new(O::translate))
            .await?;
        Ok(())
    }

    // Links with a "Broker": any message received by self::Input will be forwarded to the
    // Broker, and any Broker::Output message will be forwarded to self::Output.
    pub async fn link_broker<IO: Async + Clone + fmt::Debug + 'static>(
        &mut self,
        mut broker: broker::Broker<IO>,
    ) -> Result<(), BrokerError>
    where
        I: TranslateFrom<IO>,
        O: TranslateInto<IO>,
    {
        let link = BrokerLink {
            brokerio: self.clone(),
            broker: broker.clone(),
        };
        broker.add_handler(Box::new(link.clone())).await?;
        self.add_translator(Box::new(link)).await?;
        Ok(())
    }

    /// Waits for all messages in the queue to be forwarded / handled, before returning.
    /// It also calls all brokers that are signed up as forwarding targets.
    /// The caller argument is to be used when recursively settling, to avoid
    /// endless loops.
    pub async fn settle(&mut self, callers: Vec<BrokerID>) -> Result<(), BrokerError> {
        let (tx, mut rx) = unbounded_channel();
        self.intern_tx
            .send(InternMessage::Settle(callers.clone(), tx))
            .map_err(|_| BrokerError::SendQueue("settle".into()))?;
        rx.recv().await;
        Ok(())
    }
}

#[cfg(target_family = "wasm")]
impl<I: 'static + Message, O: 'static + Message> BrokerIO<I, O> {
    pub async fn add_handler(
        &mut self,
        handler: Box<dyn SubsystemHandler<I, O>>,
    ) -> Result<usize, BrokerError> {
        self.add_subsystem(Subsystem::Handler(handler)).await
    }

    pub async fn add_translator(
        &mut self,
        translate: Box<dyn SubsystemTranslator<I, O>>,
    ) -> Result<usize, BrokerError> {
        self.add_subsystem(Subsystem::Translator(translate)).await
    }
}
#[cfg(target_family = "unix")]
impl<I: 'static + Message, O: 'static + Message> BrokerIO<I, O> {
    pub async fn add_handler(
        &mut self,
        handler: Box<dyn SubsystemHandler<I, O> + Send>,
    ) -> Result<usize, BrokerError> {
        self.add_subsystem(Subsystem::Handler(handler)).await
    }

    pub async fn add_translator(
        &mut self,
        translate: Box<dyn SubsystemTranslator<I, O> + Send>,
    ) -> Result<usize, BrokerError> {
        self.add_subsystem(Subsystem::Translator(translate)).await
    }
}


enum InternMessage<I: Message, O: Message> {
    Subsystem(SubsystemAction<I, O>),
    MessageIn(Destination, I),
    MessageOut(Destination, O),
    Settle(Vec<BrokerID>, UnboundedSender<bool>),
}

struct Intern<I: Message, O: Message> {
    main_rx: UnboundedReceiver<InternMessage<I, O>>,
    subsystems: HashMap<usize, Subsystem<I, O>>,
    msg_queue_in: Vec<(Destination, I)>,
    msg_queue_out: Vec<(Destination, O)>,
    id: BrokerID,
}

impl<I: Message + 'static, O: Message + 'static> Intern<I, O> {
    fn new(id: BrokerID) -> UnboundedSender<InternMessage<I, O>> {
        log::trace!(
            "Creating BrokerIO {} for {}->{}",
            id,
            std::any::type_name::<I>(),
            std::any::type_name::<O>()
        );
        let (main_tx, main_rx) = unbounded_channel::<InternMessage<I, O>>();
        spawn_local(async move {
            let mut intern = Self {
                main_rx,
                subsystems: HashMap::new(),
                msg_queue_in: vec![],
                msg_queue_out: vec![],
                id,
            };
            loop {
                if !intern.get_msg().await {
                    log::warn!(
                        "{}: Closed Intern.main_rx for {}",
                        intern.id,
                        intern.type_id()
                    );
                    return;
                }

                if !intern.msg_queue_in.is_empty() || !intern.msg_queue_out.is_empty() {
                    match intern.process().await {
                        Ok(nbr) => log::trace!("{}: Processed {nbr} messages", intern.type_id()),
                        Err(e) => {
                            log::error!("{}: Couldn't process: {e:?}", intern.type_id());
                        }
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
                return false;
            }
        };
        match msg_queue {
            InternMessage::Subsystem(ss) => {
                log::trace!("{self:p}/{} subsystem action {ss:?}", self.type_id());
                self.subsystem_action(ss);
                return true;
            }
            InternMessage::MessageIn(dst, msg) => self.msg_queue_in.push((dst, msg)),
            InternMessage::MessageOut(dst, msg) => self.msg_queue_out.push((dst, msg)),
            InternMessage::Settle(list, reply) => {
                let type_id = self.type_id();
                if !list.contains(&self.id) {
                    let mut list = list.clone();
                    list.push(self.id);
                    for (_, ss) in self.subsystems.iter_mut() {
                        ss.settle(list.clone())
                            .await
                            .err()
                            .map(|e| log::error!("{}: While settling: {e:?}", type_id));
                    }
                }
                reply
                    .send(true)
                    .err()
                    .map(|e| log::error!("{}: Couldn't send: {e:?}", type_id));
                return true;
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
    async fn process_once(&mut self) -> Result<(), BrokerError> {
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
                    self.type_id()
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
                    Subsystem::TranslatorCallback(translator) => {
                        translator(trail.clone(), msg.clone()).await;
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
                    self.type_id()
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

        let type_id = self.type_id();
        for (i, ss) in self.subsystems.iter_mut() {
            if let Err(e) = ss.send_tap_out(msgs.clone()).await {
                log::warn!(
                    "{}: Couldn't send to tap: {e:?} - perhaps you should use 'remove_subsystem'?",
                    type_id
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

        let type_id = self.type_id();
        for (i, ss) in self.subsystems.iter_mut() {
            if let Err(e) = ss.send_tap_in(msgs.clone()).await {
                log::warn!(
                    "{}: Couldn't send to tap: {e:?} - perhaps you should use 'remove_subsystem'?",
                    type_id
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
        let type_id = self.type_id();
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
                    log::error!("{}: While sending messages: {e}", type_id);
                }
            }
        }
        self.msg_queue_out.append(&mut new_msg_queue);

        ss_remove
    }

    fn type_id(&self) -> String {
        format!(
            "{}->{}",
            std::any::type_name::<I>(),
            std::any::type_name::<O>()
        )
    }
}

#[cfg(target_family = "wasm")]
/// Subsystems available in a brokerio.
/// Every subsystem can be added zero, one, or more times.
pub enum Subsystem<I, O> {
    TapIn(UnboundedSender<I>),
    TapOut(UnboundedSender<O>),
    TapSyncIn(Sender<I>),
    TapSyncOut(Sender<O>),
    Handler(Box<dyn SubsystemHandler<I, O>>),
    Translator(Box<dyn SubsystemTranslator<I, O>>),
    Callback(SubsystemCallback<I, O>),
    TranslatorCallback(SubsystemTranslatorCallback<O>),
}
#[cfg(target_family = "unix")]
/// Subsystems available in a brokerio.
/// Every subsystem can be added zero, one, or more times.
pub enum Subsystem<I, O> {
    TapIn(UnboundedSender<I>),
    TapOut(UnboundedSender<O>),
    TapSyncIn(Sender<I>),
    TapSyncOut(Sender<O>),
    Handler(Box<dyn SubsystemHandler<I, O> + Send>),
    Translator(Box<dyn SubsystemTranslator<I, O> + Send>),
    Callback(SubsystemCallback<I, O>),
    TranslatorCallback(SubsystemTranslatorCallback<O>),
}

impl<I: Message, O: Message> Subsystem<I, O> {
    async fn send_tap_out(&mut self, msgs: Vec<O>) -> Result<(), BrokerError> {
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

    async fn send_tap_in(&mut self, msgs: Vec<I>) -> Result<(), BrokerError> {
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
    ) -> Result<Vec<(Destination, O)>, BrokerError> {
        Ok(match self {
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

    fn is_translator(&self) -> bool {
        matches!(self, Self::Translator(_)) || matches!(self, Self::TranslatorCallback(_))
    }

    fn is_handler(&self) -> bool {
        matches!(self, Self::Handler(_)) || matches!(self, Self::Callback(_))
    }

    async fn settle(&mut self, callers: Vec<BrokerID>) -> Result<(), BrokerError> {
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
            Self::TranslatorCallback(_) => write!(f, "TranslatorCallback"),
            Self::Callback(_) => write!(f, "Callback"),
        }
    }
}

#[platform_async_trait()]
pub trait SubsystemHandler<I: Async, O: Async> {
    async fn messages(&mut self, from_broker: Vec<I>) -> Vec<O>;
}

#[platform_async_trait()]
pub trait SubsystemTranslator<I: Async, O: Async> {
    async fn translate_input(&mut self, trail: Vec<BrokerID>, from_broker: I);
    async fn translate_output(&mut self, trail: Vec<BrokerID>, from_broker: O);
    async fn settle(&mut self, callers: Vec<BrokerID>) -> Result<(), BrokerError>;
}

#[cfg(target_family = "wasm")]
type SubsystemCallback<I, O> = Box<dyn Fn(Vec<I>) -> BoxFuture<'static, Vec<(Destination, O)>>>;
#[cfg(target_family = "unix")]
type SubsystemCallback<I, O> =
    Box<dyn Fn(Vec<I>) -> BoxFuture<'static, Vec<(Destination, O)>> + Send + Sync>;
#[cfg(target_family = "wasm")]
type SubsystemTranslatorCallback<T> = Box<dyn Fn(Vec<BrokerID>, T) -> BoxFuture<'static, bool>>;
#[cfg(target_family = "unix")]
type SubsystemTranslatorCallback<T> =
    Box<dyn Fn(Vec<BrokerID>, T) -> BoxFuture<'static, bool> + Send + Sync>;

#[cfg(target_family = "wasm")]
/// Defines a method that translates from message type <R> to message
/// type <T>.
pub type Translate<A, B> = Box<dyn Fn(A) -> Option<B> + 'static>;
#[cfg(target_family = "unix")]
/// Defines a method that translates from message type <A> to message
/// type <B>.
pub type Translate<A, B> = Box<dyn Fn(A) -> Option<B> + Send + 'static>;

struct Translator<I: Message, O: Message, TI: Message + 'static, TO: Message + 'static> {
    brokerio: BrokerIO<TI, TO>,
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
                .brokerio
                .emit_msg_in_dest(Destination::Forwarded(trail.clone()), msg_tr)
            {
                log::error!("Translated message couldn't be queued: {e}",);
            };
        }
        if let Some(msg_tr) = msg_to {
            if let Err(e) = self
                .brokerio
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

    async fn settle(&mut self, callers: Vec<BrokerID>) -> Result<(), BrokerError> {
        if !callers.contains(&self.brokerio.id) {
            self.brokerio.settle(callers).await?;
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

#[derive(Clone)]
struct BrokerLink<I: Message, O: Message, IO: Async + Clone + fmt::Debug> {
    broker: broker::Broker<IO>,
    brokerio: BrokerIO<I, O>,
}

#[platform_async_trait()]
impl<
        I: Message + TranslateFrom<IO> + 'static,
        O: Message + 'static,
        IO: Async + Clone + fmt::Debug,
    > broker::SubsystemHandler<IO> for BrokerLink<I, O, IO>
{
    async fn messages(&mut self, from_broker: Vec<IO>) -> Vec<IO> {
        for msg in from_broker {
            if let Some(msg_in) = TranslateFrom::translate(msg) {
                if let Err(e) = self.brokerio.emit_msg_in(msg_in) {
                    log::warn!("Couldn't convert message: {e:?}");
                }
            }
        }
        vec![]
    }
}

#[platform_async_trait()]
impl<
        I: Message + 'static,
        O: Message + TranslateInto<IO> + 'static,
        IO: Async + Clone + fmt::Debug + 'static,
    > SubsystemTranslator<I, O> for BrokerLink<I, O, IO>
{
    async fn translate_input(&mut self, _: Vec<BrokerID>, _: I) {}

    async fn translate_output(&mut self, _: Vec<BrokerID>, msg: O) {
        if let Some(msg_in) = msg.translate() {
            self.broker.emit_msg(msg_in).unwrap();
        }
    }

    async fn settle(&mut self, callers: Vec<BrokerID>) -> Result<(), BrokerError> {
        if !callers.contains(&self.brokerio.id) {
            self.brokerio.settle(callers.clone()).await?;
            self.broker.settle(callers).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use broker::BrokerError;
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

    /// Test the brokerio with two subsystems.
    #[tokio::test]
    async fn test_broker_new() -> Result<(), BrokerError> {
        start_logging();

        let bm_a = BrokerTestIn::MsgA;
        let bm_b = BrokerTestOut::MsgB;

        let brokerio = &mut BrokerIO::new();
        // Add a first subsystem that will reply 'msg_b' when it
        // receives 'msg_a'.
        brokerio
            .add_subsystem(Subsystem::Handler(Box::new(Tps {
                reply: vec![(bm_a.clone(), bm_b.clone())],
            })))
            .await?;
        let (tap, _) = brokerio.get_tap_out_sync().await?;

        // Should reply to msg_a, so the tap should have 1 message - the original
        // and the reply.
        brokerio.settle_msg_in(bm_a.clone()).await?;
        assert_eq!(tap.try_iter().count(), 1);

        // Add the same subsystem, now it should get 2 messages - the two replies.
        brokerio
            .add_subsystem(Subsystem::Handler(Box::new(Tps {
                reply: vec![(bm_a.clone(), bm_b)],
            })))
            .await?;
        brokerio.settle_msg_in(bm_a).await?;
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
        Un,
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
        BrokerIO(#[from] BrokerError),
    }

    #[tokio::test]
    async fn link() -> Result<(), ConvertError> {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let mut broker_a: BrokerIO<MessageAI, MessageAO> = BrokerIO::new();
        let (tap_a_in, _) = broker_a.get_tap_in_sync().await?;
        let mut broker_b: BrokerIO<MessageBI, MessageBO> = BrokerIO::new();
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
            return Err(ConvertError::Conversion("A to B".to_string()));
        }

        broker_b.settle_msg_out(MessageBO::Un).await?;
        if let Ok(msg) = tap_a_in.try_recv() {
            assert_eq!(MessageAI::One, msg);
        } else {
            return Err(ConvertError::Conversion("B to A".to_string()));
        }

        broker_a.settle_msg_out(MessageAO::Trois).await?;
        // Remove the untranslated message
        assert!(tap_b_in.try_recv().is_err());
        broker_b.settle_msg_out(MessageBO::Trois).await?;
        // Remove the untranslated message
        assert!(tap_a_in.try_recv().is_err());

        Ok(())
    }

    async fn do_something(from_broker: Vec<MessageAI>) -> Vec<(Destination, MessageAO)> {
        from_broker
            .iter()
            .filter(|msg| msg == &&MessageAI::One)
            .map(|_| (Destination::All, MessageAO::Un))
            .collect()
    }

    #[tokio::test]
    async fn test_callback() -> Result<(), Box<dyn std::error::Error>> {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let mut b = BrokerIO::new();
        b.add_subsystem(Subsystem::Callback(Box::new(|b_in| {
            Box::pin(do_something(b_in))
        })))
        .await?;
        let tap = b.get_tap_out_sync().await?;
        b.settle_msg_in_dest(Destination::NoTap, MessageAI::One)
            .await?;
        assert_eq!(MessageAO::Un, tap.0.recv()?);
        Ok(())
    }

    #[derive(Clone, std::fmt::Debug, PartialEq)]
    enum MessageAIO {
        Input(MessageAI),
        Output(MessageAO),
    }

    impl TranslateInto<MessageAIO> for MessageAO {
        fn translate(self) -> Option<MessageAIO> {
            Some(match self {
                MessageAO::Un => MessageAIO::Input(MessageAI::One),
                MessageAO::Deux => MessageAIO::Input(MessageAI::_Two),
                MessageAO::Trois => MessageAIO::Input(MessageAI::_Three),
            })
        }
    }

    impl TranslateFrom<MessageAIO> for MessageAI {
        fn translate(value: MessageAIO) -> Option<MessageAI> {
            if let MessageAIO::Output(out) = value {
                Some(match out {
                    MessageAO::Un => MessageAI::One,
                    MessageAO::Deux => MessageAI::_Two,
                    MessageAO::Trois => MessageAI::_Three,
                })
            } else {
                None
            }
        }
    }

    #[tokio::test]
    async fn test_broker() -> Result<(), BrokerError> {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let mut bio = BrokerIO::<MessageAI, MessageAO>::new();
        let mut b = broker::Broker::<MessageAIO>::new();
        bio.link_broker(b.clone()).await?;

        let (bio_in, _) = bio.get_tap_in_sync().await?;
        let (b_tap, _) = b.get_tap_sync().await?;

        bio.settle_msg_out(MessageAO::Un).await?;
        assert_eq!(
            MessageAIO::Input(MessageAI::One),
            b_tap.recv().expect("receive b_tap")
        );

        b.settle_msg(MessageAIO::Output(MessageAO::Deux)).await?;
        assert_eq!(MessageAI::_Two, bio_in.recv().expect("receive bio_out"));
        Ok(())
    }
}
