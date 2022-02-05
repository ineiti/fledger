global.WebSocket = require('ws');
global.Window = function(){};
const wrtc = require('wrtc');
global.RTCPeerConnection = wrtc.RTCPeerConnection;
global.RTCIceCandidate = wrtc.RTCIceCandidate;
global.fs = require('fs');
require("../static/wasm.js");
function wait10s(){
  console.log(new Date());
  setTimeout(() => wait10s(), 10000);
}
wait10s();
