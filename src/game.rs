use crate::net;
use crate::prediction;
use crate::types::*;

use chrono::Duration;
use nalgebra_glm::Vec3;
use rayon::prelude::*;
use tokio::{net::TcpStream, sync::RwLock};
use tokio_serde::{formats::Json, Framed};
use tokio_util::codec::{Framed as CodecFramed, LengthDelimitedCodec};
use tracing::{debug, error, info, warn};

use std::net::SocketAddr;

pub async fn advance_simulation(state: State, _delta_time: Duration) -> Result<()> {
    let predictions;
    let look_behind = Duration::hours(2);
    let spacing = Duration::minutes(5);
    let num_predictions = 18;

    // Check for new data, update the entity positions with that.
    {
        let mut entities = state.entities.write().await;
        let mut history = state.history.write().await;

        history.iter_mut().for_each(|(k, v)| {
            if v.was_updated {
                v.was_updated = false;
                let latest = v.history.last().unwrap();

                entities
                    .entry(k.clone())
                    .and_modify(|e| *e = latest.clone())
                    .or_insert(latest.clone());
            }

            debug!("New data arrived, updated entity {}", k);
        });
    }

    {
        let entities;

        // Copy the entities to avoid a deadlock.
        {
            let _entities = state.entities.read().await;
            entities = _entities.clone();
        }

        debug!("Running {} entity predictions", entities.len());

        // If we've run out predictions for an entity, go ahead and generate more.
        predictions = entities
            .par_iter()
            .map(|(k, v)| {
                let state = state.clone();
                let k = k.clone();
                let v = v.clone();

                if v.prediction_cache.is_empty() {
                    Some(prediction::predict_entity(
                        state.clone(),
                        k,
                        look_behind,
                        spacing,
                        num_predictions,
                        prediction::PredictionKind::GeneticAlgorithm,
                    ))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            // Remove the None values.
            .into_iter()
            .filter_map(|e| e)
            .collect::<Vec<_>>();
    }

    {
        debug!("Updating entity prediction caches");
        let mut entities = state.entities.write().await;
        let history = state.history.read().await;
        let mut anomalies = futures::executor::block_on(state.anomalies.write());

        // Cache the predictions.
        for i in predictions.iter() {
            match i {
                Ok((entity_id, entity_predictions)) => {
                    let entity = entities
                        .get_mut(&entity_id.clone())
                        .expect("Enable to find entity");

                    for e in entity_predictions {
                        entity.prediction_cache.push_back(e.clone());
                    }
                }
                Err(e) => error!("Predictions failed: {}", e),
            }
        }

        debug!("Updating entity positions");

        // Use the predictions to set the entity position.
        entities.iter_mut().for_each(|(k, v)| {
            let current_time = chrono::offset::Utc::now();

            // Check the staleness of the next prediction.
            if !v.prediction_cache.is_empty() {
                let prediction = v
                    .prediction_cache
                    .front()
                    .clone()
                    .expect("Empty prediction cache");

                // The prediction is too old, remove it.
                if prediction.date_time > current_time {
                    v.prediction_cache
                        .pop_front()
                        .expect("Prediction cache was empty?");
                }
            }

            match v.prediction_cache.front() {
                Some(e) => {
                    let interpolated_prediction = prediction::interpolate_cached_prediction(
                        e.clone(),
                        spacing,
                        v.position,
                        current_time.clone(),
                    );

                    // Update the position from the cached prediction.
                    v.position = interpolated_prediction.position;

                    if !is_position_valid(v.position) {
                        warn!("Entity {} is in an invalid position {}", k, v.position);
                    }

                    let entity_history = history.get(k).expect("Missing entity id").clone();

                    // Check for anomalies.
                    if prediction::is_entity_in_anomalous_position(
                        v.clone(),
                        entity_history.history.clone(),
                    ) {
                        info!("Detected position anomaly on entity {}", k);
                        anomalies.push(Anomaly {
                            entity_id: k.clone(),
                            msg: "Anomalous position detected".to_owned(),
                        });
                    }
                }
                None => (),
            }
        });
    }

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

fn is_position_valid(position: Vec3) -> bool {
    const MIN_X: f32 = -75.0;
    const MAX_X: f32 = 163.0;
    const MIN_Z: f32 = -70.31605;
    const MAX_Z: f32 = -2.001192;

    position.x >= MIN_X && position.x <= MAX_X && position.z >= MIN_Z && position.z <= MAX_Z
}
