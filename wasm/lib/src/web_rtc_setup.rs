use async_trait::async_trait;

use std::sync::{Arc, Mutex};

use js_sys::Reflect;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use common::signal::web_rtc::{ConnectionStateMap ,WebRTCConnectionSetup, WebRTCConnectionState, WebRTCSetupCB, WebRTCSetupCBMessage};

use web_sys::{
    console::log_1, Event, RtcDataChannel, RtcDataChannelEvent, RtcIceCandidate,
    RtcIceCandidateInit, RtcPeerConnection, RtcPeerConnectionIceEvent, RtcSdpType,
    RtcSessionDescriptionInit, RtcSignalingState,
};

use crate::web_rtc_connection::{WebRTCConnectionWasm, get_state};

fn log(s: &str) {
    log_1(&JsValue::from_str(s));
}

/// Structure for easy WebRTC handling without all the hassle of JS-internals.
pub struct WebRTCConnectionSetupWasm {
    nt: WebRTCConnectionState,
    rp_conn: RtcPeerConnection,
    callback: Arc<Mutex<Option<WebRTCSetupCB>>>,
}

impl WebRTCConnectionSetupWasm {
    /// Returns a new WebRTCConnection in either init or follower mode.
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
    pub fn new(nt: WebRTCConnectionState) -> Result<Box<dyn WebRTCConnectionSetup>, String> {
        let rp_conn =
            RtcPeerConnection::new().map_err(|e| format!("PeerConnection error: {:?}", e))?;
        let rn = WebRTCConnectionSetupWasm {
            nt,
            rp_conn: rp_conn.clone(),
            callback: Arc::new(Mutex::new(None)),
        };
        ice_start(&rp_conn, Arc::clone(&rn.callback));
        let cb_clone = Arc::clone(&rn.callback);
        match nt {
            WebRTCConnectionState::Initializer => dc_create_init(rp_conn.clone(), cb_clone),
            WebRTCConnectionState::Follower => dc_create_follow(rp_conn.clone(), cb_clone),
        };
        Ok(Box::new(rn))
    }

    // Making sure the struct is in correct state

    fn is_initializer(&self) -> Result<(), String> {
        if self.nt != WebRTCConnectionState::Initializer {
            return Err("This method is only available to the Initializer".to_string());
        }
        Ok(())
    }

    fn is_follower(&self) -> Result<(), String> {
        if self.nt != WebRTCConnectionState::Follower {
            return Err("This method is only available to the Follower".to_string());
        }
        Ok(())
    }
}

#[async_trait(?Send)]
impl WebRTCConnectionSetup for WebRTCConnectionSetupWasm {
    // Returns the offer string that needs to be sent to the `Follower` node.
    async fn make_offer(&mut self) -> Result<String, String> {
        self.is_initializer()?;
        let offer = JsFuture::from(self.rp_conn.create_offer())
            .await
            .map_err(|e| e.as_string().unwrap())?;
        let offer_sdp = Reflect::get(&offer, &JsValue::from_str("sdp"))
            .map_err(|e| e.as_string().unwrap())?
            .as_string()
            .unwrap();

        let mut offer_obj = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
        offer_obj.sdp(&offer_sdp);
        let sld_promise = self.rp_conn.set_local_description(&offer_obj);
        JsFuture::from(sld_promise)
            .await
            .map_err(|e| e.as_string().unwrap())?;
        Ok(offer_sdp)
    }

    // Takes the offer string
    async fn make_answer(&mut self, offer: String) -> Result<String, String> {
        self.is_follower()?;
        let mut offer_obj = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
        offer_obj.sdp(&offer);
        let srd_promise = self.rp_conn.set_remote_description(&offer_obj);
        JsFuture::from(srd_promise)
            .await
            .map_err(|e| e.as_string().unwrap())?;

        let answer = match JsFuture::from(self.rp_conn.create_answer()).await {
            Ok(f) => f,
            Err(e) => {
                log(&format!("Error answer: {:?}", e));
                return Err(e.as_string().unwrap());
            }
        };
        let answer_sdp = Reflect::get(&answer, &JsValue::from_str("sdp"))
            .map_err(|e| e.as_string().unwrap())?
            .as_string()
            .unwrap();

        let mut answer_obj = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
        answer_obj.sdp(&answer_sdp);
        let sld_promise = self.rp_conn.set_local_description(&answer_obj);
        JsFuture::from(sld_promise)
            .await
            .map_err(|e| e.as_string().unwrap())?;
        Ok(answer_sdp)
    }

    // Takes the answer string and finalizes the first part of the connection.
    async fn use_answer(&mut self, answer: String) -> Result<(), String> {
        self.is_initializer()?;
        if self.rp_conn.signaling_state() == RtcSignalingState::Stable{
            return Ok(());
        }
        let mut answer_obj = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
        answer_obj.sdp(&answer);
        let srd_promise = self.rp_conn.set_remote_description(&answer_obj);
        JsFuture::from(srd_promise)
            .await
            .map_err(|e| format!("{:?}", e))?;
        Ok(())
    }

    // Waits for the ICE string to be avaialble.
    async fn set_callback(&mut self, cb: WebRTCSetupCB) {
        self.callback.lock().unwrap().replace(cb);
    }

    // Sends the ICE string to the WebRTC.
    async fn ice_put(&mut self, ice: String) -> Result<(), String> {
        // self.is_not_setup()?;
        let rp_clone = self.rp_conn.clone();
        let els: Vec<&str> = ice.split(":-:").collect();
        if els.len() != 3 || els[0] == "" {
            log(&format!("wrong ice candidate string: {}", ice));
            return Ok(());
        }
        let mut ric_init = RtcIceCandidateInit::new(els[0]);
        ric_init.sdp_mid(Some(els[1]));
        ric_init.sdp_m_line_index(Some(els[2].parse::<u16>().unwrap()));
        match RtcIceCandidate::new(&ric_init) {
            Ok(e) => {
                // DEBUG: clean up logging once ICE stuff is more clear...
                // log(&format!("dbg: Adding ice: {:?} / {:?}", els, e));
                if let Err(e) = wasm_bindgen_futures::JsFuture::from(
                    rp_clone.add_ice_candidate_with_opt_rtc_ice_candidate(Some(&e)),
                )
                .await
                {
                    log(&format!("Couldn't add ice candidate: {:?}", e));
                // } else {
                //     log("dbg: Added ice successfully");
                }
                Ok(())
            }
            Err(e) => Err(format!("Couldn't consume ice: {:?}", e)),
        }
        .map_err(|js| js.to_string())
    }

    async fn get_state(&self) -> Result<ConnectionStateMap, String> {
        get_state(self.rp_conn.clone()).await
    }
}

fn ice_start(rp_conn: &RtcPeerConnection, callback: Arc<Mutex<Option<WebRTCSetupCB>>>) {
    let onicecandidate_callback1 =
        Closure::wrap(
            Box::new(move |ev: RtcPeerConnectionIceEvent| match ev.candidate() {
                Some(candidate) => {
                    let cand = format!(
                        "{}:-:{}:-:{}",
                        candidate.candidate(),
                        candidate.sdp_mid().unwrap(),
                        candidate.sdp_m_line_index().unwrap()
                    );
                    if let Some(cb) = callback.lock().unwrap().as_ref() {
                        cb(WebRTCSetupCBMessage::Ice(cand.clone()));
                    }
                }
                None => {}
            }) as Box<dyn FnMut(RtcPeerConnectionIceEvent)>,
        );
    rp_conn.set_onicecandidate(Some(onicecandidate_callback1.as_ref().unchecked_ref()));
    onicecandidate_callback1.forget();
}

fn dc_create_init(rp_conn: RtcPeerConnection, cb: Arc<Mutex<Option<WebRTCSetupCB>>>) {
    let dc = rp_conn.create_data_channel("data-channel");
    dc_set_onopen(&mut Some(dc), &mut Some(rp_conn), cb);
}

fn dc_create_follow(rp_conn: RtcPeerConnection, cb: Arc<Mutex<Option<WebRTCSetupCB>>>) {
    let mut rpc = Some(rp_conn.clone());
    let ondatachannel_callback = Closure::wrap(Box::new(move |ev: RtcDataChannelEvent| {
        let dc = ev.channel();
        dc_set_onopen(&mut Some(dc), &mut rpc, Arc::clone(&cb));
    }) as Box<dyn FnMut(RtcDataChannelEvent)>);
    rp_conn.set_ondatachannel(Some(ondatachannel_callback.as_ref().unchecked_ref()));
    ondatachannel_callback.forget();
}

fn dc_set_onopen(
    dc: &mut Option<RtcDataChannel>,
    rp_conn: &mut Option<RtcPeerConnection>,
    cb: Arc<Mutex<Option<WebRTCSetupCB>>>,
) {
    let dcc = dc.take().unwrap();
    let mut dccc = Some(dcc.clone());
    let mut rpc = Some(rp_conn.take().unwrap());
    let ondatachannel_open = Closure::wrap(Box::new(move |_ev: Event| {
        let conn = WebRTCConnectionWasm::new(dccc.take().unwrap(), rpc.take().unwrap());
        cb.lock().unwrap().as_ref().unwrap()(WebRTCSetupCBMessage::Connection(conn));
    }) as Box<dyn FnMut(Event)>);
    dcc.set_onopen(Some(ondatachannel_open.as_ref().unchecked_ref()));
    ondatachannel_open.forget();
}
