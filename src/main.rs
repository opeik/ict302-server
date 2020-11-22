mod api;
mod auth;
mod game;
mod net;
mod prediction;
mod types;

use types::*;

use std::clone::Clone;

use chrono::Duration;
use tokio::{net::TcpListener, time::delay_for};
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
    match api::fetch_entities(state.clone(), Duration::days(2)).await {
        Ok(_) => info!("Fetched initial entity data from API"),
        Err(e) => error!("Failed to fetch data from API: {}", e),
    };

    // Run each task concurrently, stopping all if any return.
    let status = tokio::select! {
        e = tokio::task::spawn(connection_handler(state.clone())) => e.unwrap(),
        e = tokio::task::spawn(game_loop(state.clone())) => e.unwrap(),
    };

    match status {
        Ok(_) => panic!("Server exiting normally...?"),
        Err(e) => error!("Error occurred: {:?}", e),
    }

    Ok(())
}

async fn connection_handler(state: State) -> Result<()> {
    debug!("Connection handler task started");
    let host = "::";
    let port = 42069;
    let mut listener = TcpListener::bind(format!("{}:{}", host, port)).await?;
    info!("Listening for connections on [{}]:{}...", host, port);

    loop {
        let (stream, addr) = listener.accept().await?;
        game::new_player(state.clone(), stream, addr).await?;
    }
}

async fn game_loop(state: State) -> Result<()> {
    loop {
        let delta_time = Duration::seconds(1);
        delay_for(delta_time.to_std().unwrap()).await;
        debug!("Game loop tick");

        // Fetch data.
        match api::fetch_entities(state.clone(), Duration::minutes(2)).await {
            Ok(_) => info!("Fetched entity data from API"),
            Err(e) => error!("Failed to fetch data from API: {}", e),
        };

        // Update all entities.
        game::advance_simulation(state.clone(), delta_time).await?;
        // Send updates to players.
        game::send_updates(state.clone()).await?;
        // Send updates from players.
        game::receive_updates(state.clone()).await?;

        let mut anomalies = state.anomalies.write().await;
        anomalies.clear();
    }
}
