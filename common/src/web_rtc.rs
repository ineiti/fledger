use std::future::Future;

use crate::ext_interface::WebRTCCall;

pub struct WebRTCClient<F>
where
    F: Future,
{
    rtc: WebRTCCall<F>,
}

impl<F> WebRTCClient<F>
where
    F: Future<Output = Result<Option<String>, String>>,
{
    pub fn new(rtc: WebRTCCall<F>) -> WebRTCClient<F> {
        WebRTCClient { rtc }
    }

    /// Returns the offer string that needs to be sent to the `Follower` node.
    pub fn make_offer(&self) -> Result<String, String> {
        Err("Not implemented".to_string())
    }

    /// Takes the offer string
    pub fn make_answer(&self, offer: String) -> Result<String, String> {
        Err("Not implemented".to_string())
    }

    /// Takes the answer string and finalizes the first part of the connection.
    pub fn use_answer(&self, answer: String) -> Result<(), String> {
        Err("Not implemented".to_string())
    }

    /// Waits for the ICE to move on from the 'New' state
    pub fn wait_gathering(&self) -> Result<(), String> {
        Err("Not implemented".to_string())
    }

    /// Waits for the ICE string to be avaialble.
    pub fn ice_string(&self) -> Result<String, String> {
        Err("Not implemented".to_string())
    }

    /// Sends the ICE string to the WebRTC.
    pub fn ice_put(&self, ice: String) -> Result<(), String> {
        Err("Not implemented".to_string())
    }

    /// Waits for a message to arrive. If no message arrives within 10 * 100ms,
    /// an error is returned.
    pub fn msg_receive(&mut self) -> Result<String, String> {
        Err("Not implemented".to_string())
    }

    /// Sends a message to the other end. If the DataChannel is not set up yet,
    /// it needs to wait for it to happen. If the setup doesn't happen within 10 * 1s,
    /// an error is returned.
    pub fn msg_send(&mut self, s: &str) -> Result<(), String> {
        Err("Not implemented".to_string())
    }

    /// Debugging output of the RTC state
    pub fn print_states(&self) {
        panic!("Not implemented");
    }
}
