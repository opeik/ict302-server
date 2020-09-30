mod api;
mod types;

use types::*;

use chrono::Duration;
use float_duration::FloatDuration;
use futures::sink::SinkExt;
use nalgebra_glm::*;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio::time::delay_for;
use tokio_serde::{formats::*, SymmetricallyFramed};
use tokio_util::codec::{FramedWrite, LengthDelimitedCodec};
use tracing::{debug, error, info};
use tracing_subscriber;

use std::collections::HashMap;
use std::io::Error;
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<()> {
    // Start the tracing library.
    tracing_subscriber::fmt::init();

    info!("Server starting...");

    // Setup the initial server state.
    let state = State::new();

    // Fetch the initial entity state from the API.
    match api::fetch_entities(state.clone(), Duration::days(1)).await {
        Ok(_) => info!("Fetched initial entity data from API"),
        Err(e) => error!("Failed to fetch data from API: {}", e),
    };

    // Run each task concurrently, stopping all if any return.
    let status = tokio::select! {
        e = tokio::spawn(connection_handler(state.clone())) => e.unwrap(),
        e = tokio::spawn(game_loop(state.clone())) => e.unwrap(),
        e = tokio::spawn(data_fetcher(state.clone())) => e.unwrap(),
    };

    match status {
        Ok(_) => panic!("Server exiting normally...?"),
        Err(e) => error!("Error occurred: {:?}", e),
    }

    Ok(())
}

async fn connection_handler(state: State) -> Result<()> {
    info!("Connection handler task started");
    let mut listener = TcpListener::bind("localhost:42069").await?;
    info!("Listening for connections...");

    loop {
        let (stream, addr) = listener.accept().await?;
        info!("Client connected from {:?}", addr);
        let connection = Connection::new(stream, addr);

        {
            let mut connections = state.connections.write().await;
            let mut player_count = state.player_count.write().await;
            let new_player_id = *player_count;
            *player_count += 1;
            connections.insert(new_player_id, connection);
        }
    }
}

async fn game_loop(state: State) -> Result<()> {
    loop {
        let delta_time = Duration::seconds(1);
        let delta_time_float = FloatDuration::from_chrono(delta_time).as_seconds() as f32;
        delay_for(delta_time.to_std().unwrap()).await;
        debug!("Game loop tick");

        // Update entity positions.
        {
            let mut entities = state.entities.write().await;

            entities.iter_mut().for_each(|(_, v)| {
                v.position = v.position + (v.velocity * delta_time_float);
            });
        }
    }
}

async fn data_fetcher(state: State) -> Result<()> {
    loop {
        delay_for(Duration::minutes(1).to_std().unwrap()).await;
        debug!("Data fetcher tick");

        match api::fetch_entities(state.clone(), Duration::minutes(2)).await {
            Ok(_) => info!("Fetched entity data from API"),
            Err(e) => error!("Failed to fetch data from API: {}", e),
        };
    }
}
//async fn handle_client(
//    state: Arc<Mutex<State>>,
//    stream: TcpStream,
//    addr: SocketAddr,
//) -> Result<()> {
//    // Delimit frames using a length header
//    let length_delimited = FramedWrite::new(stream, LengthDelimitedCodec::new());
//    // Serialize frames with JSON
//    let mut client = SymmetricallyFramed::new(length_delimited, SymmetricalJson::default());
//
//    let state = state.lock().await;
//
//    if let Err(e) = client.send(&state.entities).await {
//        error!("Failed to send JSON to client {:?}: {:?}", addr, e);
//    }
//
//    Ok(())
//}
//
