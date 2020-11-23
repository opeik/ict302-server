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

use std::net::SocketAddr;

const MAX_REGENERATION_ATTEMPTS: i32 = 3;

pub async fn advance_simulation(state: State, _delta_time: Duration) -> Result<()> {
    let predictions;
    let look_behind = Duration::hours(2);
    let spacing = Duration::minutes(5);
    let num_predictions = 18;

    // Check for new data, update the entity positions with that.
    {
        let mut entities = state.entities.write().await;
        let mut history = state.history.write().await;
        let mut anomalies = state.anomalies.write().await;

        history.iter_mut().for_each(|(k, v)| {
            if v.was_updated {
                v.was_updated = false;
                let latest = v.history.last().unwrap();
                let entity = entities.get(&k.clone());

                match entity {
                    Some(e) => {
                        let is_anomalous = is_position_anomalous(
                            e.position,
                            latest.position,
                            e.prediction_cache
                                .front()
                                .expect("Missing predictions")
                                .error,
                            e.num_stale_predictions,
                        );

                        if is_anomalous {
                            info!("Detected position anomaly on entity {}", k);
                            anomalies.push(Anomaly {
                                entity_id: k.clone(),
                                msg: "Anomalous position detected".to_owned(),
                            });
                        }
                    }
                    None => {}
                }

                entities
                    .entry(k.clone())
                    .and_modify(|e| *e = latest.clone())
                    .or_insert(latest.clone());
            }
        });
    }

    {
        let entities;

        // Copy the entities to avoid a deadlock.
        {
            let _entities = state.entities.read().await;
            entities = _entities.clone();
        }

        debug!("Running entity predictions");

        // If we've run out predictions for an entity, go ahead and generate more.
        predictions = entities
            .par_iter()
            .map(|(k, v)| {
                let state = state.clone();
                let k = k.clone();
                let v = v.clone();

                if v.prediction_cache.is_empty() {
                    // Try the genetic algorithm.
                    for _ in 0..MAX_REGENERATION_ATTEMPTS {
                        let result = prediction::predict_entity(
                            state.clone(),
                            k.clone(),
                            look_behind,
                            spacing,
                            num_predictions,
                            prediction::PredictionKind::GeneticAlgorithm,
                        );

                        // Check if the values are sane, if they aren't, try regenerating.
                        match result {
                            Ok((_, ref _predictions)) => {
                                let predictions_sane =
                                    are_predictions_sane(k.clone(), state.clone());

                                match predictions_sane {
                                    Ok(_) => {
                                        return Some(result);
                                    }
                                    Err(_) => panic!(
                                        "Unable to check predictions were sane, what the hell?"
                                    ),
                                }
                            }
                            Err(_e) => (),
                        }
                    }

                    // Screw it, use linear extrapolation.
                    Some(prediction::predict_entity(
                        state.clone(),
                        k.clone(),
                        look_behind,
                        spacing,
                        num_predictions,
                        prediction::PredictionKind::LinearExtrapolation,
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
                Err(_) => {}
            }
        }

        debug!("Updating {} entity positions", entities.len());

        // Use the predictions to set the entity position.
        entities.iter_mut().for_each(|(_, v)| {
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
                    v.num_stale_predictions += 1;
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
                    v.prediction_kind = Some(e.kind);
                }
                None => (),
            }
        });
    }

    // Blasted deadlocks.
    {
        let mut players = state.players.write().await;

        players.iter_mut().for_each(|(_, v)| {
            if v.is_authenticated && !v.did_send_auth_status {
                v.did_send_auth_status = true;
            }
        })
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

fn are_predictions_sane(entity_id: String, state: State) -> Result<bool> {
    const MAGIC: f32 = 3.0;

    let history = futures::executor::block_on(state.history.read());
    let entities = futures::executor::block_on(state.entities.read());
    let entity_history = history.get(&entity_id).expect("Missing entity id");
    let entity = entities
        .get(&entity_id.clone())
        .expect("Enable to find entity");

    let history_x_min = entity_history
        .history
        .iter()
        .map(|e| e.position.x)
        .fold(f32::INFINITY, |a, b| a.min(b));

    let history_x_max = entity_history
        .history
        .iter()
        .map(|e| e.position.x)
        .fold(f32::INFINITY, |a, b| a.max(b));

    let history_z_min = entity_history
        .history
        .iter()
        .map(|e| e.position.z)
        .fold(f32::INFINITY, |a, b| a.min(b));

    let history_z_max = entity_history
        .history
        .iter()
        .map(|e| e.position.z)
        .fold(f32::INFINITY, |a, b| a.max(b));

    let predictions_x_min = entity
        .prediction_cache
        .iter()
        .map(|e| e.position.x)
        .fold(f32::INFINITY, |a, b| a.min(b));

    let predictions_x_max = entity
        .prediction_cache
        .iter()
        .map(|e| e.position.x)
        .fold(f32::INFINITY, |a, b| a.max(b));

    let predictions_z_min = entity
        .prediction_cache
        .iter()
        .map(|e| e.position.z)
        .fold(f32::INFINITY, |a, b| a.min(b));

    let predictions_z_max = entity
        .prediction_cache
        .iter()
        .map(|e| e.position.z)
        .fold(f32::INFINITY, |a, b| a.max(b));

    if (predictions_x_max - predictions_x_min) > MAGIC * (history_x_max - history_x_min)
        && (predictions_z_max - predictions_z_min) > MAGIC * (history_z_max - history_z_min)
    {
        Ok(true)
    } else {
        Ok(false)
    }
}

fn is_position_anomalous(
    current_position: Vec3,
    actual_position: Vec3,
    error: Vec3,
    staleness: i32,
) -> bool {
    const CONFIDENCE_INTERVAL_MULTIPLIER: f32 = 2.58;
    const STEP_EXPONENT: f32 = 0.9;

    let factor = (staleness as f32).powf(STEP_EXPONENT);
    let bounds_x = CONFIDENCE_INTERVAL_MULTIPLIER * error.x * factor;
    let bounds_z = CONFIDENCE_INTERVAL_MULTIPLIER * error.z * factor;

    if actual_position.x >= (current_position.x - bounds_x)
        && actual_position.x <= (current_position.x + bounds_x)
        && actual_position.z >= (current_position.x - bounds_z)
        && actual_position.z <= (current_position.x + bounds_z)
    {
        false
    } else {
        true
    }
}
