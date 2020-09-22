use std::io::{self, prelude::*};
use std::net::TcpListener;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;

mod types;
use types::types::*;

fn main() {
    start_server();
}

fn start_server() {
    let state = Arc::new(Mutex::new(State::new()));

    let group_id = state
        .lock()
        .unwrap()
        .push_group("users", vec![Permission::Login, Permission::GetEntityData]);

    state
        .lock()
        .unwrap()
        .push_user("opeik", "bruz", vec![group_id]);
    state.lock().unwrap().push_entity([0.0; 3], [0.0; 3]);

    let listener = TcpListener::bind("127.0.0.1:42069").unwrap();
    println!("Server started...");

    for stream in listener.incoming() {
        let stream = stream.unwrap();
        let state = state.clone();

        thread::spawn(move || {
            handle_connection(state, stream).unwrap();
        });
    }
}

fn handle_connection(state: Arc<Mutex<State>>, mut stream: TcpStream) -> io::Result<()> {
    let s = state.lock().unwrap();

    let json = serde_json::to_string(&s.entities).unwrap();
    stream.write(json.as_bytes())?;

    Ok(())
}
