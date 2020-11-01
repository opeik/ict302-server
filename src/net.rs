use crate::types::*;

use futures::SinkExt;
use tokio::stream::StreamExt;
use tracing::{debug, error, info};

pub async fn send_update(state: State, player_id: PlayerId) -> Result<()> {
    let entities = state.entities.read().await;
    let players = state.players.read().await;

    let connections = state.connections.read().await;
    let mut connection = connections
        .get(&player_id)
        .ok_or(Error::PlayerNotFound { player_id })?
        .write()
        .await;

    let server_update_data = ServerUpdateData {
        entities: entities.clone(),
        players: players.clone(),
    };

    let message = ServerMessage::Update(server_update_data);

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

    Ok(())
}

pub async fn receive_update(state: State, player_id: PlayerId) -> Result<()> {
    let connections = state.connections.read().await;
    let mut connection = connections
        .get(&player_id)
        .ok_or(Error::PlayerNotFound { player_id })?
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
                    let player = players
                        .get_mut(&player_id)
                        .ok_or(Error::PlayerNotFound { player_id })?;
                    player.position = v.position;
                }
                ClientMessage::Disconnect => {
                    info!("Player {} disconnected", player_id);
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
