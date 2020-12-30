use super::logs;

use crate::logs::wait_ms;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, SyncSender};

use js_sys::Reflect;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use web_sys::{
    MessageEvent, RtcDataChannel, RtcDataChannelEvent, RtcDataChannelState, RtcIceCandidate,
    RtcIceCandidateInit, RtcIceGatheringState, RtcPeerConnection, RtcPeerConnectionIceEvent,
    RtcSdpType, RtcSessionDescriptionInit,
};

pub async fn demo() -> Result<(), JsValue> {
    // Set up two PCs - one needs to have init == true, the other init == false
    let mut pc1 = RtcNode::new(RtcNodeState::Initializer)?;
    let mut pc2 = RtcNode::new(RtcNodeState::Follower)?;

    // Exchange SDP info - 'offer' and 'answer' are strings that need to be exchanged over a
    // signalling server.
    console_log!("Sending out offer and answer");
    let offer = pc1.make_offer().await?;
    let answer = pc2.make_answer(offer).await?;
    pc1.use_answer(answer).await?;

    // Now both nodes need to wait for the messages to be exchanged.
    pc1.wait_gathering().await?;
    pc2.wait_gathering().await?;

    // Same thing for the ICE information that is converted to strings here and must be passed
    // through a signnalling server.
    console_log!("Pass ICE back and forth");
    let r1_str = pc1.ice_string().await?;
    pc2.ice_put(r1_str)?;
    let r2_str = pc2.ice_string().await?;
    pc1.ice_put(r2_str)?;

    // Finally the two nodes are set up and can exchange messages.
    console_log!("Sending something through the channel");
    for i in 0..2 {
        console_log!("Doing iteration {}", i);
        pc1.msg_send("1 -> 2").await?;
        console_log!("PC2 receives: {}", pc2.msg_receive().await?);
        pc2.msg_send("2 -> 1").await?;
        console_log!("PC1 receives: {}", pc1.msg_receive().await?);
    }

    console_log!("Going away - all done");
    Ok(())
}

/// Structure for easy WebRTC handling without all the hassle of JS-internals.
pub struct RtcNode {
    nt: RtcNodeState,
    rp_conn: RtcPeerConnection,
    ch_msg: Receiver<String>,
    ch_dc: Receiver<RtcDataChannel>,
    ch_ice: Receiver<String>,
    dc: Option<RtcDataChannel>,
}

/// What type of node this is
#[derive(PartialEq, Debug)]
pub enum RtcNodeState {
    Initializer,
    Follower,
}

impl RtcNode {
    /// Returns a new RtcNode in either init or follower mode.
    /// One of the nodes connecting must be an init node, the other a follower node.
    /// There is no error handling if both are init nodes or both are follower nodes.
    ///
    /// # Arguments
    ///
    /// * `init` - Initializer or Follower
    ///
    /// # Actions
    ///
    /// Once two nodes are set up, they need to exchang the offer and the answer string.
    /// Followed by that they need to exchange the ice strings, in either order.
    /// Only after exchanging this information can the msg_send and msg_receive methods be used.
    pub fn new(nt: RtcNodeState) -> Result<RtcNode, JsValue> {
        let rp_conn = RtcPeerConnection::new()?;
        let (ch_msg_send, ch_msg) = mpsc::sync_channel::<String>(1);
        let ch_dc = match nt {
            RtcNodeState::Initializer => dc_create_init(&rp_conn, &ch_msg_send)?,
            RtcNodeState::Follower => dc_create_follow(&rp_conn, &ch_msg_send),
        };
        let ch_ice = ice_start(&rp_conn);
        let rn = RtcNode {
            nt,
            rp_conn,
            ch_msg,
            ch_dc,
            ch_ice,
            dc: None,
        };
        Ok(rn)
    }

    /// Returns the offer string that needs to be sent to the `Follower` node.
    pub async fn make_offer(&self) -> Result<String, JsValue> {
        self.is_initializer()?;
        let offer = JsFuture::from(self.rp_conn.create_offer()).await?;
        let offer_sdp = Reflect::get(&offer, &JsValue::from_str("sdp"))?
            .as_string()
            .unwrap();

        let mut offer_obj = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
        offer_obj.sdp(&offer_sdp);
        let sld_promise = self.rp_conn.set_local_description(&offer_obj);
        JsFuture::from(sld_promise).await?;
        Ok(offer_sdp)
    }

    /// Takes the offer string
    pub async fn make_answer(&self, offer: String) -> Result<String, JsValue> {
        self.is_follower()?;
        let mut offer_obj = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
        offer_obj.sdp(&offer);
        let srd_promise = self.rp_conn.set_remote_description(&offer_obj);
        JsFuture::from(srd_promise).await?;

        let answer = match JsFuture::from(self.rp_conn.create_answer()).await {
            Ok(f) => f,
            Err(e) => {
                console_log!("Error answer: %{:?}", e);
                return Err(e);
            }
        };
        let answer_sdp = Reflect::get(&answer, &JsValue::from_str("sdp"))?
            .as_string()
            .unwrap();

        let mut answer_obj = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
        answer_obj.sdp(&answer_sdp);
        let sld_promise = self.rp_conn.set_local_description(&answer_obj);
        JsFuture::from(sld_promise).await?;
        Ok(answer_sdp)
    }

    /// Takes the answer string and finalizes the first part of the connection.
    pub async fn use_answer(&self, answer: String) -> Result<(), JsValue> {
        self.is_initializer()?;
        let mut answer_obj = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
        answer_obj.sdp(&answer);
        let srd_promise = self.rp_conn.set_remote_description(&answer_obj);
        JsFuture::from(srd_promise).await?;
        Ok(())
    }

    /// Waits for the ICE to move on from the 'New' state
    pub async fn wait_gathering(&self) -> Result<(), JsValue> {
        self.is_not_setup()?;
        for _ in 0u8..10 {
            match self.rp_conn.ice_gathering_state() {
                RtcIceGatheringState::New => wait_ms(1000).await,
                _ => return Ok(()),
            }
        }
        Err(JsValue::from_str("Didn't reach IceGatheringState"))
    }

    /// Waits for the ICE string to be avaialble.
    pub async fn ice_string(&self) -> Result<String, JsValue> {
        self.is_not_setup()?;
        for _ in 0..10 {
            match self.ch_ice.try_iter().next() {
                Some(s) => {
                    return Ok(s);
                }
                None => (),
            };
            wait_ms(1000).await;
        }
        Err(JsValue::from_str("Didn't get ICE in time"))
    }

    /// Sends the ICE string to the WebRTC.
    pub fn ice_put(&self, ice: String) -> Result<(), JsValue> {
        self.is_not_setup()?;
        let rp_clone = self.rp_conn.clone();
        let els: Vec<&str> = ice.split("::").collect();
        if els.len() != 3 {
            return Err(JsValue::from_str("wrong ice candidate string"));
        }
        let mut ric_init = RtcIceCandidateInit::new(els[0]);
        ric_init.sdp_mid(Some(els[1]));
        ric_init.sdp_m_line_index(Some(els[2].parse::<u16>().unwrap()));
        match RtcIceCandidate::new(&ric_init) {
            Ok(e) => {
                let _ = rp_clone.add_ice_candidate_with_opt_rtc_ice_candidate(Some(&e));
                Ok(())
            }
            Err(e) => Err(JsValue::from_str(
                format!("Couldn't consume ice: {:?}", e).as_str(),
            )),
        }
    }

    /// Waits for a message to arrive. If no message arrives within 10 * 100ms,
    /// an error is returned.
    pub async fn msg_receive(&mut self) -> Result<String, JsValue> {
        self.is_open().await?;
        for _ in 0..10 {
            match self.ch_msg.try_iter().next() {
                Some(s) => {
                    console_log!("Got message: {}", s);
                    return Ok(s);
                }
                None => (),
            };
            wait_ms(100).await;
        }
        Err(JsValue::from_str("Couldn't get string in time"))
    }

    /// Sends a message to the other end. If the DataChannel is not set up yet,
    /// it needs to wait for it to happen. If the setup doesn't happen within 10 * 1s,
    /// an error is returned.
    pub async fn msg_send(&mut self, s: &str) -> Result<(), JsValue> {
        self.is_open().await?;
        console_log!("Sending: {}", s);
        match &self.dc {
            Some(dc) => dc.send_with_str(s),
            None => Err(JsValue::from_str("Didn't get a DataChannel")),
        }
    }

    pub fn print_states(&self) {
        console_log!(
            "{:?}: rpc_conn state is: {:?} / {:?} / {:?}",
            self.nt,
            self.rp_conn.signaling_state(),
            self.rp_conn.ice_gathering_state(),
            self.rp_conn.ice_connection_state()
        );
    }

    // Making sure the struct is in correct state

    fn is_initializer(&self) -> Result<(), JsValue> {
        if self.nt != RtcNodeState::Initializer {
            return Err(JsValue::from_str(
                "This method is only available to the Initializer",
            ));
        }
        Ok(())
    }

    fn is_follower(&self) -> Result<(), JsValue> {
        if self.nt != RtcNodeState::Follower {
            return Err(JsValue::from_str(
                "This method is only available to the Follower",
            ));
        }
        Ok(())
    }

    fn is_not_setup(&self) -> Result<(), JsValue> {
        if self.dc.is_some() {
            return Err(JsValue::from_str(
                "This method is only available before setup is complete",
            ));
        }
        Ok(())
    }

    async fn is_setup(&mut self) -> Result<(), JsValue> {
        if self.dc.is_none() {
            self.is_not_setup()?;
            console_log!("Waiting for RtcDataChannel");
            for _ in 0u8..10 {
                console_log!("ch_dc.try_iter");
                match self.ch_dc.try_iter().next() {
                    Some(dc) => {
                        console_log!("Found RDC: {:?}", dc.ready_state());
                        self.dc = Some(dc);
                        return Ok(());
                    }
                    None => wait_ms(1000).await,
                }
                console_log!("ch_dc.try_iter loop");
            }
            return Err(JsValue::from_str(
                "This method is only available once setup is complete",
            ));
        }
        Ok(())
    }

    async fn is_open(&mut self) -> Result<(), JsValue> {
        self.is_setup().await?;
        for _ in 0u8..10 {
            match &self.dc {
                Some(dc) => {
                    if dc.ready_state() == RtcDataChannelState::Open {
                        return Ok(());
                    }
                }
                None => return Err(JsValue::from_str("DataChannel should be set by now!")),
            }
            wait_ms(1000).await;
        }
        Err(JsValue::from_str(
            "DataChannelState is not going into 'Open'",
        ))
    }
}

fn ice_start(rp_conn: &RtcPeerConnection) -> Receiver<String> {
    let (s1, r1) = mpsc::sync_channel::<String>(1);

    let onicecandidate_callback1 = Closure::wrap(Box::new(move |ev: RtcPeerConnectionIceEvent| {
        console_log!("Going for ICE: {:?}", ev);
        match ev.candidate() {
            Some(candidate) => {
                let cand = format!(
                    "{}::{}::{}",
                    candidate.candidate(),
                    candidate.sdp_mid().unwrap(),
                    candidate.sdp_m_line_index().unwrap()
                );
                match s1.try_send(cand) {
                    Ok(_) => (),
                    Err(e) => console_log!("Couldn't transmit ICE string: {:?}", e),
                }
            }
            None => {}
        }
    })
        as Box<dyn FnMut(RtcPeerConnectionIceEvent)>);
    rp_conn.set_onicecandidate(Some(onicecandidate_callback1.as_ref().unchecked_ref()));
    onicecandidate_callback1.forget();
    r1
}

fn dc_onmessage(dc: &RtcDataChannel, s: &SyncSender<String>) {
    let sender = s.clone();
    let onmessage_callback = Closure::wrap(Box::new(move |ev: MessageEvent| {
        console_log!("New event: {:?}", ev);
        match ev.data().as_string() {
            Some(message) => {
                console_log!("got: {:?}", message);
                match sender.send(message) {
                    Ok(()) => (),
                    Err(e) => console_log!("Error while sending: {}", e),
                }
            }
            None => {}
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    dc.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
    onmessage_callback.forget();
}

fn dc_create_init(
    rp_conn: &RtcPeerConnection,
    send: &SyncSender<String>,
) -> Result<Receiver<RtcDataChannel>, JsValue> {
    let dc = rp_conn.create_data_channel("data-channel");
    dc_onmessage(&dc.clone(), send);
    let (dc_send, dc_rcv) = mpsc::sync_channel::<RtcDataChannel>(1);
    dc_send
        .send(dc)
        .map_err(|e| JsValue::from_str(e.to_string().as_str()))?;
    Ok(dc_rcv)
}

fn dc_create_follow(
    rp_conn: &RtcPeerConnection,
    s: &SyncSender<String>,
) -> Receiver<RtcDataChannel> {
    let (dc_send, dc_rcv) = mpsc::sync_channel::<RtcDataChannel>(1);
    let send = s.clone();
    let ondatachannel_callback = Closure::wrap(Box::new(move |ev: RtcDataChannelEvent| {
        let dc = ev.channel();
        console_log!(
            "ondatachannel: {:?} in state {:?}",
            dc.label(),
            dc.ready_state()
        );
        dc_onmessage(&dc, &send);
        match dc_send.send(dc) {
            Err(e) => console_warn!("Error while sending dc: {}", e),
            _ => (),
        };
    }) as Box<dyn FnMut(RtcDataChannelEvent)>);
    rp_conn.set_ondatachannel(Some(ondatachannel_callback.as_ref().unchecked_ref()));
    ondatachannel_callback.forget();
    dc_rcv
}
