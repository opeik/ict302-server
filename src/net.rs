use crate::auth;
use crate::types::*;

use futures::SinkExt;
use tokio::stream::StreamExt;
use tracing::{debug, error, info};

pub async fn send_update(state: State, player_id: PlayerId) -> Result<()> {
    let entities = state.entities.read().await;
    let players = state.players.read().await;
    let player = players.get(&player_id).ok_or("Player not found")?;
    let anomalies = state.anomalies.read().await;

    let connections = state.connections.read().await;
    let mut connection = connections
        .get(&player_id)
        .ok_or("Player not found")?
        .write()
        .await;

    let message;

    if player.is_authenticated {
        if player.did_send_auth_status {
            let data = ServerUpdateData {
                player_id: player_id,
                entities: entities.clone(),
                players: players.clone(),
                anomalies: anomalies.clone(),
            };

            message = ServerMessage::Update(data);
        } else {
            let data = ServerAuthenticateResponseData {
                success: true,
                msg: "Logged in".to_owned(),
            };

            message = ServerMessage::AuthenticateResponse(data);
        }
    } else {
        let data = ServerAuthenticateResponseData {
            success: false,
            msg: "Invalid credentials".to_owned(),
        };

        message = ServerMessage::AuthenticateResponse(data);
    }

    if let Err(e) = connection.framed.send(message.clone()).await {
        error!(
            "Error occurred while sending update to player {} at {:?}: {:?}",
            player_id, connection.addr, e
        );

        return Err(Box::new(e));
    }

    debug!(
        "Sent message to player {} at {:?}",
        player_id, connection.addr
    );

    // Close unauthenticated connections.
    if !player.is_authenticated {
        connection.framed.close().await?;
    }

    Ok(())
}

pub async fn receive_update(state: State, player_id: PlayerId) -> Result<()> {
    let connections = state.connections.read().await;
    let mut connection = connections
        .get(&player_id)
        .ok_or("Player not found")?
        .write()
        .await;

    let result = connection.framed.next().await;

    if let Some(message) = result {
        info!(
            "Received message from player {} at {:?}",
            player_id, connection.addr
        );

        match message {
            Ok(value) => match value {
                ClientMessage::Update(v) => {
                    let mut players = state.players.write().await;
                    let player = players.get_mut(&player_id).ok_or("Player not found")?;
                    player.position = v.position;
                    player.angles = v.angles;
                }
                ClientMessage::Disconnect => {
                    info!("Player {} disconnected", player_id);
                }
                ClientMessage::Authenticate(v) => {
                    let mut players = state.players.write().await;
                    let player = players.get_mut(&player_id).ok_or("Player not found")?;
                    let is_authenticated = auth::authenticate_user(&v.username, &v.password);

                    match is_authenticated {
                        Ok(_) => {
                            player.is_authenticated = true;
                            info!(
                                "Player {} with name {} authenticated",
                                player_id, v.username
                            );
                        }
                        Err(_) => {
                            info!(
                                "Player {} with name {} failed to authenticate",
                                player_id, v.username
                            );
                        }
                    }
                }
            },
            Err(e) => error!("Failed to parse message: {:?}", e),
        }
    } else {
        error!(
            "Failed to read message from player {} at {:?}",
            player_id, connection.addr
        );
    }

    Ok(())
}

pub async fn reap_dead_connection(state: State, player_id: PlayerId, did_fail: bool) {
    if !did_fail {
        return;
    }

    let mut connections = state.connections.write().await;
    let mut players = state.players.write().await;

    info!(
        "Player {} has a broken socket, forcing disconnect",
        player_id
    );

    connections.remove(&player_id);
    players.remove(&player_id);
}
