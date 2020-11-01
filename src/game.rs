use std::net::SocketAddr;

use crate::net;
use crate::types::*;

use chrono::Duration;
use float_duration::FloatDuration;
use nalgebra_glm::Vec3;
use tokio::{net::TcpStream, sync::RwLock};
use tokio_serde::{formats::Json, Framed};
use tokio_util::codec::{Framed as CodecFramed, LengthDelimitedCodec};
use tracing::{debug, info};

pub async fn advance_simulation(state: State, delta_time: Duration) -> Result<()> {
    // Update entity positions.
    let delta_time_float = FloatDuration::from_chrono(delta_time).as_seconds() as f32;
    let mut entities = state.entities.write().await;
    debug!("Updating {} entity positions", entities.len());

    entities.iter_mut().for_each(|(_, v)| {
        v.position = v.position + (v.velocity * delta_time_float);
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
    futures::future::join_all(handles).await;

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
    players.insert(new_player_id, Player::new(Vec3::new(0.0, 0.0, 0.0)));
    info!("Player {} connected from {:?}", new_player_id, addr);

    Ok(())
}
