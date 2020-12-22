#![feature(proc_macro_hygiene, decl_macro)]
#[macro_use]
extern crate rocket;
use rocket_contrib::json::Json;
use rocket::local::Client;

use common::*;

#[get("/listID", format = "json")]
fn list_id() -> Json<GetListID> {
    Json(GetListID{
        new_id: [0; 32],
    })
}

#[get("/list", format = "json")]
fn list() -> Json<GetWebRTC> {
    Json(GetWebRTC {
        list: Box::new([PostWebRTC {
            list_id: [0; 32],
            webrtc_id: "webrtcID".to_string(),
            node_id: "nodeID".to_string(),
        }]),
    })
}

fn main() {
    let rocket = rocket::ignite().mount("/a", routes![list, list_id]);
    let client = Client::new(rocket).expect("valid rocket instance");
    let req = client.get("/a/listID");
    let mut response = req.dispatch();
    println!("{:?}", response.body_string());
}
