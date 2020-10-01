mod api;
mod game;
mod net;
mod types;

use types::*;

use chrono::Duration;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio::time::delay_for;
use tokio_serde::{formats::Json, Framed};
use tokio_util::codec::{Framed as CodecFramed, LengthDelimitedCodec};
use tracing::{debug, error, info};
use tracing_subscriber;

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
    debug!("Connection handler task started");
    let mut listener = TcpListener::bind("localhost:42069").await?;
    info!("Listening for connections...");

    loop {
        let (stream, addr) = listener.accept().await?;
        info!("Client connected from {:?}", addr);

        let length_delimited = CodecFramed::new(stream, LengthDelimitedCodec::new());
        let message_framed: MessageFramed = Framed::new(length_delimited, Json::default());
        let connection = Connection::new(message_framed, addr);

        {
            let mut connections = state.connections.write().await;
            let mut player_count = state.player_count.write().await;
            let new_player_id = *player_count;
            *player_count += 1;
            connections.insert(new_player_id, RwLock::new(connection));
        }
    }
}

async fn game_loop(state: State) -> Result<()> {
    loop {
        let delta_time = Duration::seconds(1);
        delay_for(delta_time.to_std().unwrap()).await;
        debug!("Game loop tick");

        // Update all entities.
        game::advance_simulation(state.clone(), delta_time).await?;

        // Send entity data to players.
        {
            let connections = state.connections.read().await;
            debug!("Sending data to {} players", connections.len());

            let handles = connections
                .iter()
                .map(|(i, _)| tokio::spawn(net::send_update(state.clone(), *i)))
                .collect::<Vec<_>>();

            futures::future::join_all(handles).await;
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
