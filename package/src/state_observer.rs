use flarch::broker::Broker;
use serde::Serialize;
use tokio::sync::watch;
use wasm_bindgen::prelude::wasm_bindgen;
use web_sys::ReadableStream;

use crate::{
    error::WasmResult,
    proxy::state::{State, StateUpdate},
};

#[derive(Debug, Serialize, Clone)]
struct StateUpdateMsg {
    update: StateUpdate,
    state: State,
}

#[wasm_bindgen]
pub struct StateObserver {
    state: Broker<StateUpdate, ()>,
    snapshot: watch::Receiver<State>,
}

#[wasm_bindgen]
impl StateObserver {
    pub async fn listen_updates(&mut self) -> WasmResult<ReadableStream> {
        let mut b: Broker<StateUpdateMsg, ()> = Broker::new();
        let (stream, _) = b.get_async_iterable_in().await?;
        let snapshot = self.snapshot.clone();
        self.state
            .add_translator_i_ti(
                b,
                Box::new(move |update| {
                    Some(StateUpdateMsg {
                        state: snapshot.borrow().clone(),
                        update,
                    })
                }),
            )
            .await?;
        Ok(stream)
    }
}

impl StateObserver {
    pub fn start(state: Broker<StateUpdate, ()>, snapshot: watch::Receiver<State>) -> Self {
        Self { state, snapshot }
    }
}
