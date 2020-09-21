use serde::{Deserialize, Serialize};
use std::io::{self, prelude::*};
use std::net::TcpListener;
use std::net::TcpStream;
use std::thread;

#[derive(Serialize, Deserialize)]
struct Entity {
    id: i32,
    position: [f32; 3],
    angles: [f32; 4],
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();
    println!("Server started...");

    for stream in listener.incoming() {
        let stream = stream.unwrap();

        thread::spawn(|| {
            handle_connection(stream).unwrap();
        });
    }
}

fn handle_connection(mut stream: TcpStream) -> io::Result<()> {
    let entity = Entity {
        id: 0,
        position: [0.0; 3],
        angles: [0.0; 4],
    };

    let json = serde_json::to_string(&entity).unwrap();
    stream.write(json.as_bytes())?;

    Ok(())
}
