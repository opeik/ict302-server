use crate::types::*;

use futures::SinkExt;
use tracing::{debug, error, info};

pub async fn send_update(state: State, player_id: PlayerId) -> Result<()> {
    let entities = state.entities.read().await;
    let connections = state.connections.read().await;
    let mut connection = connections
        .get(&player_id)
        .ok_or(Error::PlayerNotFound { player_id })?
        .write()
        .await;
    let message = ServerMessage::Update(entities.clone());

    if let Err(e) = connection.framed.send(message.clone()).await {
        error!(
            "Error occurred while sending update to player {} at {:?}: {:?}",
            player_id, &connection.addr, e
        );

        return Err(Box::new(e));
    }

    debug!(
        "Sent update to player {} at {:?}",
        player_id, &connection.addr
    );

    Ok(())
}

pub async fn reap_dead_connection(state: State, player_id: PlayerId, did_fail: bool) {
    if !did_fail {
        return;
    }

    let mut connections = state.connections.write().await;

    info!(
        "Player {} has a broken socket, forcing disconnect",
        player_id
    );

    connections.remove(&player_id);
}
