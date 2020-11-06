mod api;
mod game;
mod net;
mod prediction;
mod types;

use types::*;

use std::clone::Clone;

use chrono::Duration;
use std::thread;
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
    match api::fetch_entities(state.clone(), Duration::days(1)).await {
        Ok(_) => info!("Fetched initial entity data from API"),
        Err(e) => error!("Failed to fetch data from API: {}", e),
    };

    // Run each task concurrently, stopping all if any return.
    let status = tokio::select! {
        e = tokio::task::spawn(connection_handler(state.clone())) => e.unwrap(),
        e = tokio::task::spawn(data_fetcher(state.clone())) => e.unwrap(),
        e = {
            let state = state.clone();
            tokio::task::spawn_blocking(move || game_loop(state.clone()) )
        } => e.unwrap().await,
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

        // Update all entities.
        game::advance_simulation(state.clone(), delta_time).await?;
        // Send updates to players.
        game::send_updates(state.clone()).await?;
        // Send updates from players.
        game::receive_updates(state.clone()).await?;
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
