use crate::broker::{BInput, Broker, BrokerMessage};

#[derive(Debug, Clone, PartialEq)]
pub enum BrokerTimer {
    Second,
    Minute,
}

/// The Timer structure sends out periodic signals to the system so that
/// services can subscribe to them.
pub struct Timer {
    seconds: u32,
    broker: Broker,
}

#[cfg(target_arch = "wasm32")]
pub fn schedule_repeating<F>(cb: F)
where
    F: 'static + FnMut() + Send,
{
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        pub fn setInterval(callback: JsValue, millis: u32) -> f64;
    }
    let ccb = Closure::wrap(Box::new(cb) as Box<dyn FnMut()>);
    setInterval(ccb.into_js_value(), 1000);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn schedule_repeating<F>(cb: F)
where
    F: 'static + FnMut() + Send,
{
    let timer = timer::Timer::new();
    timer.schedule_repeating(chrono::Duration::seconds(1), cb);
}

impl Timer {
    pub fn new(broker: Broker) {
        let mut timer_struct = Timer { seconds: 0, broker };
        schedule_repeating(move || timer_struct.process());
    }

    fn emit(&mut self, msg: BrokerTimer) {
        if let Err(e) = self
            .broker
            .emit(vec![BInput::BM(BrokerMessage::Timer(msg.clone()))])
        {
            log::warn!("While emitting {:?}, got error: {:?}", msg, e);
        }
    }

    fn process(&mut self) {
        self.emit(BrokerTimer::Second);
        if self.seconds == 0 {
            self.emit(BrokerTimer::Minute);
            self.seconds = 60;
        }
        self.seconds = self.seconds - 1;
    }
}
