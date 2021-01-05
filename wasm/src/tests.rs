use common::config::NodeInfo;
use common::ext_interface::WebRTCCallerState;

use common::rest::RestClient;
use common::web_rtc::WebRTCClient;
use wasm_bindgen::prelude::*;

use crate::rest::RestCallerWasm;
use crate::web_rtc::WebRTCCallerWasm;
pub async fn test_webrtc() -> Result<(), JsValue> {
    // Set up two PCs - one needs to have init == true, the other init == false
    let pc1 = WebRTCCallerWasm::new(WebRTCCallerState::Initializer)?;
    let pc2 = WebRTCCallerWasm::new(WebRTCCallerState::Follower)?;
    let mut rc1 = WebRTCClient::new(Box::new(pc1));
    let mut rc2 = WebRTCClient::new(Box::new(pc2));

    // Exchange SDP info - 'offer' and 'answer' are strings that need to be exchanged over a
    // signalling server.
    console_log!("Sending out offer and answer");
    let offer = rc1.make_offer().await?;
    let answer = rc2.make_answer(offer).await?;
    rc1.use_answer(answer).await?;

    // Now both nodes need to wait for the messages to be exchanged.
    rc1.wait_gathering().await?;
    rc2.wait_gathering().await?;

    // Same thing for the ICE information that is converted to strings here and must be passed
    // through a signnalling server.
    console_log!("Pass ICE back and forth");
    let r1_str = rc1.ice_string().await?;
    rc2.ice_put(r1_str).await?;
    let r2_str = rc2.ice_string().await?;
    rc1.ice_put(r2_str).await?;

    // Finally the two nodes are set up and can exchange messages.
    console_log!("Sending something through the channel");
    for i in 0..2 {
        console_log!("Doing iteration {}", i);
        rc1.msg_send("1 -> 2".to_string()).await?;
        console_log!("PC2 receives: {}", rc2.msg_receive().await?);
        rc2.msg_send("2 -> 1".to_string()).await?;
        console_log!("PC1 receives: {}", rc1.msg_receive().await?);
    }

    console_log!("Going away - all done");
    Ok(())
}

pub async fn demo() -> Result<(), JsValue> {
    console_log!("Starting REST-test in 2021");
    let rc = RestCallerWasm::new("http://localhost:8000");
    let mut rest = RestClient::new(Box::new(rc));
    rest.clear_nodes().await?;
    let id1 = rest.new_id().await?;
    let ni1 = NodeInfo::new();
    rest.add_id(id1, &ni1).await?;
    console_log!("Added new node");
    console_log!("IDs: {:?}", rest.list_ids().await?);

    Ok(())
}
