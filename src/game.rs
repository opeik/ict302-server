use crate::net;
use crate::prediction;
use crate::types::*;

use chrono::Duration;
use nalgebra_glm::Vec3;
use rayon::prelude::*;
use tokio::{net::TcpStream, sync::RwLock};
use tokio_serde::{formats::Json, Framed};
use tokio_util::codec::{Framed as CodecFramed, LengthDelimitedCodec};
use tracing::{debug, info};

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::thread;

pub async fn advance_simulation(state: State, delta_time: Duration) -> Result<()> {
    let mut prediction_handles;

    {
        // Update entity positions.
        let entities = state.entities.read().await;

        // Update the current time.
        {
            let mut current_time = state.current_time.write().await;
            *current_time = *current_time + delta_time;
        }

        let current_time = state.current_time.read().await;

        debug!("Updating {} entity positions", entities.len());

        // If we've run out predictions for an entity, go ahead and generate more.
        prediction_handles = entities
            .iter()
            .map(|(k, v)| {
                let state = state.clone();
                let current_time = current_time.clone();
                let k = k.clone();
                let v = v.clone();

                thread::spawn(move || async move {
                    if v.prediction_cache.is_empty() {
                        Some(
                            prediction::predict_entity(
                                state.clone(),
                                k,
                                current_time,
                                Duration::minutes(90),
                                Duration::minutes(1),
                                90,
                                prediction::PredictionKind::GeneticAlgorithm,
                            )
                            .await,
                        )
                    } else {
                        None
                    }
                })
            })
            .collect::<Vec<_>>();
    }

    for e in prediction_handles.iter() {
        e.join().unwrap().await;
    }

    // Cache the predictions.
    {
        let mut entities = state.entities.write().await;

        for (i, (_, v)) in entities.iter_mut().enumerate() {
            if v.prediction_cache.is_empty() {
                // This is awful but I don't have time to fix it, forgive me.
                let x = prediction_handles.remove(i).join().unwrap().await;
                debug!("{:?}", x);

                // v.prediction_cache = VecDeque::from(predictions);
            }
        }
    }

    // Use the predictions to set the entity position.
    // entities.iter_mut().for_each(|(_, v)| {
    //     v.position = v.prediction_cache.pop_front().unwrap();
    // });

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
