mod api;
mod game;
mod net;
mod types;

use types::*;

use std::clone::Clone;

use chrono::Duration;
use tokio::{net::TcpListener, sync::RwLock, time::delay_for};
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

    api::calc_training_data(Duration::hours(6), Duration::minutes(1)).await;

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
    let host = "localhost";
    let port = 42069;
    let mut listener = TcpListener::bind(format!("{}:{}", host, port)).await?;
    info!("Listening for connections on {}:{}...", host, port);

    loop {
        let (stream, addr) = listener.accept().await?;

        let length_delimited = CodecFramed::new(stream, LengthDelimitedCodec::new());
        let message_framed: MessageFramed = Framed::new(length_delimited, Json::default());
        let connection = Connection::new(message_framed, addr);

        {
            let mut connections = state.connections.write().await;
            let mut next_player_id = state.next_player_id.write().await;
            let new_player_id = *next_player_id;
            *next_player_id += 1;
            connections.insert(new_player_id, RwLock::new(connection));
            info!("Player {} connected from {:?}", new_player_id, addr);
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
            let cull_handles;

            {
                let connections = state.connections.read().await;
                debug!("Sending data to {} players", connections.len());

                let handles = connections
                    .iter()
                    .map(|(i, _)| tokio::spawn(net::send_update(state.clone(), *i)))
                    .collect::<Vec<_>>();

                // Wait for everything to be sent.
                let send_handles = futures::future::join_all(handles).await;

                // Keep track of transmission failures.
                cull_handles = connections
                    .iter()
                    .enumerate()
                    .map(|(i, (p, _))| {
                        tokio::spawn(net::reap_dead_connection(
                            state.clone(),
                            p.clone(),
                            send_handles.get(i).unwrap().as_ref().unwrap().is_err(),
                        ))
                    })
                    .collect::<Vec<_>>();
            }

            // Kill the dead connections once we're done sending and receiving.
            futures::future::join_all(cull_handles).await;
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
