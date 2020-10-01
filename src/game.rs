use crate::types::*;

use chrono::Duration;
use float_duration::FloatDuration;
use tracing::debug;

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
