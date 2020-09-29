use crate::types::*;

use chrono::Duration;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::info;

use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
struct Vessel {
    #[serde(rename = "craft_id")]
    id: f64,
    #[serde(rename = "lat")]
    latitude: f64,
    #[serde(rename = "lon")]
    longitude: f64,
    speed: f32,
    course: f32,
    beam: f32,
    draught: f32,
    timestamp: String,
    #[serde(rename = "type")]
    class: String,
    #[serde(rename = "subtype")]
    sub_class: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Response {
    #[serde(rename = "results")]
    vessels: Vec<Vessel>,
    #[serde(rename = "timespan")]
    time_span: f32,
    request_time: String,
    result_length: usize,
}

pub async fn fetch_entities(state: Arc<Mutex<State>>) -> Result<()> {
    let time_span: Duration = Duration::days(1);

    let url = Url::parse_with_params(
        "http://127.0.0.1:5000",
        &[("timestamp", time_span.num_minutes().to_string())],
    )?;

    let body = reqwest::get(url).await?.text().await?;
    let response: Response = serde_json::from_str(&body)?;
    let num_vessels = response.result_length;
    assert_eq!(response.vessels.len(), num_vessels);
    info!("Parsed entity data");

    let mut state = state.lock().await;
    response.vessels.into_iter().for_each(|v| {
        let id = v.id.to_string();
        state.entities.insert(
            id.clone(),
            Entity::new(&id, [v.latitude, v.longitude], v.course),
        );
    });
    assert_eq!(state.entities.len(), num_vessels);
    info!("Moved data into state");

    Ok(())
}
