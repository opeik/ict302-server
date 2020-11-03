use std::net::SocketAddr;

use crate::net;
use crate::prediction;
use crate::types::*;

use chrono::{Duration, Utc};
use nalgebra_glm::Vec3;
use tokio::{net::TcpStream, sync::RwLock};
use tokio_serde::{formats::Json, Framed};
use tokio_util::codec::{Framed as CodecFramed, LengthDelimitedCodec};
use tracing::{debug, error, info};

pub async fn advance_simulation(state: State, delta_time: Duration) -> Result<()> {
    // Update entity positions.
    let mut entities = state.entities.write().await;
    let mut current_time = state.current_time.write().await;
    *current_time = *current_time + delta_time;

    debug!("Updating {} entity positions", entities.len());

    let prediction_handles = entities
        .iter()
        .map(|(k, v)| {
            tokio::spawn(prediction::predict_entity(
                state.clone(),
                k.to_string(),
                *current_time + Duration::minutes(1),
                Duration::minutes(90),
                Duration::minutes(1),
                prediction::PredictionKind::GeneticAlgorithm,
            ))
        })
        .collect::<Vec<_>>();

    let predictions = futures::future::join_all(prediction_handles).await;

    predictions.iter().enumerate().for_each(|(i, v)| match v {
        Ok(v) => {
            let keys = entities.keys().collect::<Vec<_>>();
            let (prediction, error) = v.as_ref().unwrap();
            let key = keys.get(i).unwrap().to_string();
            let mut entity = entities.get_mut(&key).unwrap();
            entity.position = *prediction;
        }
        Err(e) => error!("Failed to predict: {}", e),
    });

    Ok(())
}

pub async fn send_updates(state: State) -> Result<()> {
    let cull_handles;

    {
        let connections = state.connections.read().await;

        if connections.len() == 0 {
            return Ok(());
        }

        debug!("Sending data to {} players", connections.len());

        // Start sending.
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

    // Kill the dead connections.
    futures::future::join_all(cull_handles).await;

    Ok(())
}

pub async fn receive_updates(state: State) -> Result<()> {
    let cull_handles;

    {
        let connections = state.connections.read().await;

        if connections.len() == 0 {
            return Ok(());
        }

        debug!("Receiving data from {} players", connections.len());

        // Start receiving.
        let handles = connections
            .iter()
            .map(|(i, _)| tokio::spawn(net::receive_update(state.clone(), *i)))
            .collect::<Vec<_>>();

        // Wait for everything to be received.
        let receive_handles = futures::future::join_all(handles).await;

        // Keep track of transmission failures.
        cull_handles = connections
            .iter()
            .enumerate()
            .map(|(i, (p, _))| {
                tokio::spawn(net::reap_dead_connection(
                    state.clone(),
                    p.clone(),
                    receive_handles.get(i).unwrap().as_ref().unwrap().is_err(),
                ))
            })
            .collect::<Vec<_>>();
    }

    futures::future::join_all(cull_handles).await;

    Ok(())
}

pub async fn new_player(state: State, stream: TcpStream, addr: SocketAddr) -> Result<()> {
    let length_delimited = CodecFramed::new(stream, LengthDelimitedCodec::new());
    let message_framed: MessageFramed = Framed::new(length_delimited, Json::default());
    let connection = Connection::new(message_framed, addr);

    let mut connections = state.connections.write().await;
    let mut players = state.players.write().await;
    let mut next_player_id = state.next_player_id.write().await;
    let new_player_id = *next_player_id;
    *next_player_id += 1;

    connections.insert(new_player_id, RwLock::new(connection));
    players.insert(
        new_player_id,
        Player::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0)),
    );
    info!("Player {} connected from {:?}", new_player_id, addr);

    Ok(())
}
