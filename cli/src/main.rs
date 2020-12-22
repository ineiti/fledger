#![feature(proc_macro_hygiene, decl_macro)]
#[macro_use]
extern crate rocket;

/// Very simple rendez-vous server that allows new nodes to send their node_info.
/// It also allows the nodes to fetch all existing node_infos of all the other nodes.
///
/// TODO: use the `newID` endpoint to authentify the nodes' public key

use common::node_list::NodeList;
use common::rest::*;



use rocket::Request;use rocket::Response;
use rocket::fairing::Fairing;
use rocket::fairing::Info;
use rocket::fairing::Kind;use rocket::http::ContentType;

use rocket::http::Header;
use rocket::http::Status;use rocket::response;
use rocket::response::Body;use rocket::{
    response::status::{Accepted, BadRequest},
    State,
};
use rocket_contrib::json::Json;

use std::io::Cursor;
use std::path::PathBuf;use std::sync::Mutex;

struct ServerState {
    list: Mutex<NodeList>,
}

#[get("/newID", format = "json")]
fn new_id(state: State<ServerState>) -> Json<GetListID> {
    Json(GetListID {
        new_id: state.list.lock().unwrap().get_new_idle(),
    })
}

#[get("/accepted", format = "json")]
fn accepted<'a>() -> response::Result<'a> {
    Response::build()
            .header(ContentType::JSON)
            .raw_header("status", "200 OK")
            .raw_body(Body::Sized(Cursor::new("Hello!"), 6))
            .ok()
}

#[get("/listIDs", format = "json")]
fn list_ids(state: State<ServerState>) -> Json<GetWebRTC> {
    Json(GetWebRTC {
        list: state.list.lock().unwrap().get_nodes(),
    })
}

#[delete("/clearNodes")]
fn clear_nodes(state: State<ServerState>) -> response::Result {
    state.list.lock().unwrap().clear_nodes();
    Response::build()
            .header(ContentType::JSON)
            .raw_header("status", "200 OK")
            .raw_body(Body::Sized(Cursor::new("Hello!"), 6))
            .ok()
}

#[post("/addNode", format = "json", data = "<node>")]
fn add_node(
    state: State<ServerState>,
    node: Json<PostWebRTC>,
) -> Result<Accepted<()>, BadRequest<String>> {
    match state.list.lock().unwrap().add_node(node.0) {
        Err(e) => return Err(BadRequest(Some(e))),
        Ok(_) => return Ok(Accepted(None)),
    }
}

#[options("/<_path..>")]
fn add_node_options<'a>(_path: PathBuf) -> Response<'a> {
    let mut res = Response::new();
    res.set_status(Status::new(200, "No Content"));
    // res.adjoin_header(ContentType::Plain);
    // res.adjoin_raw_header("Access-Control-Allow-Methods", "POST, GET, OPTIONS");
    // res.adjoin_raw_header("Access-Control-Allow-Origin", "*");
    // res.adjoin_raw_header("Access-Control-Allow-Credentials", "true");
    // res.adjoin_raw_header("Access-Control-Allow-Headers", "Content-Type");
    // res.set_sized_body(Cursor::new("Response"));
    res
}

pub struct CORS();

impl Fairing for CORS {
    fn info(&self) -> Info {
        Info {
            name: "Add CORS headers to requests",
            kind: Kind::Response
        }
    }

    fn on_response(&self, request: &Request, response: &mut Response) {
        response.set_header(Header::new("Access-Control-Allow-Origin", "*"));
        response.set_header(Header::new("Access-Control-Allow-Methods", "POST, GET, PATCH, OPTIONS, DELETE"));
        response.set_header(Header::new("Access-Control-Allow-Headers", "*"));
        response.set_header(Header::new("Access-Control-Allow-Credentials", "true"));
    }
}

fn ignite() -> rocket::Rocket {
    let nl = NodeList::new();
    let s = ServerState {
        list: Mutex::new(nl),
    };
    rocket::ignite()
        .manage(s)
        .attach(CORS())
        .mount("/", routes![list_ids, new_id, add_node, add_node_options, accepted, clear_nodes])
}

#[cfg(test)]
mod tests {
    use common::config::NodeInfo;
    use rocket::{http::ContentType, local::Client};
    use common::rest::*;

    #[test]
    fn test_add_node() {
        let rocket = super::ignite();
        let client = Client::new(rocket).expect("valid rocket instance");

        let mut response = client.get("/newID").dispatch();
        let s = response.body_string().unwrap();
        let lid: GetListID = serde_json::from_str(s.as_str()).unwrap();

        let pwr = PostWebRTC {
            list_id: lid.new_id,
            node: NodeInfo::new(),
        };
        let response = client
            .post("/addNode")
            .header(ContentType::JSON)
            .body(serde_json::to_string(&pwr).unwrap())
            .dispatch();
        println!("{:?}", response);
    }
}

fn main() {
    let server = ignite();
    server.launch();
}
