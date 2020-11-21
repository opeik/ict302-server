use crate::types::*;

use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use nalgebra_glm::{pi, Vec3};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use tracing::info;

use std::collections::VecDeque;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Vessel {
    #[serde(rename = "craft_id")]
    id: f32,
    #[serde(rename = "lat")]
    latitude: f32,
    #[serde(rename = "lon")]
    longitude: f32,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Response {
    #[serde(rename = "results")]
    vessels: Vec<Vessel>,
    #[serde(rename = "timespan")]
    time_span: f32,
    request_time: String,
    result_length: usize,
}

pub async fn fetch_entities(state: State, time_span: Duration) -> Result<()> {
    let url = Url::parse_with_params(
        "http://localhost:5000",
        &[("timespan", time_span.num_minutes().to_string())],
    )?;

    let body = reqwest::get(url).await?.text().await?;
    let response: Response = serde_json::from_str(&body)?;
    assert_eq!(response.vessels.len(), response.result_length);
    info!("Parsed {} entities from response", response.vessels.len());

    // TODO: Does this cause a deadlock?
    let mut entities = state.entities.write().await;
    let mut history = state.history.write().await;

    // Load all the entities into history.
    response.vessels.into_iter().for_each(|v| {
        let entity_id = v.id.to_string();

        history
            .entry(entity_id)
            .or_default()
            .push(new_entity_from_vessel(&v));
    });

    // Remove duplicates.
    history.iter_mut().for_each(|(_, v)| {
        v.dedup_by(|a, b| a.date_time == b.date_time);
    });

    // Sort the history by time.
    history.iter_mut().for_each(|(_, v)| {
        v.sort_by(|a, b| a.date_time.partial_cmp(&b.date_time).unwrap());
    });

    // Use the last known entity data.
    history.iter().for_each(|(k, v)| {
        let latest = v.last().unwrap();
        entities
            .entry(k.clone())
            .and_modify(|e| *e = latest.clone())
            .or_insert(latest.clone());
    });

    info!("Updated state with new entity data");

    Ok(())
}

fn knots_to_mps(knots: f32) -> f32 {
    const RATIO: f32 = 0.51444444444;

    knots * RATIO
}

fn new_entity_from_vessel(vessel: &Vessel) -> Entity {
    let radians = (vessel.course * pi::<f32>()) / 180.0;
    let speed = knots_to_mps(vessel.speed);
    let date_time = DateTime::<Utc>::from_utc(
        NaiveDateTime::parse_from_str(&vessel.timestamp, "%F %T").unwrap(),
        Utc,
    );

    Entity::new(
        Vec3::new(vessel.longitude, 0.0, vessel.latitude),
        Vec3::new(0.0, vessel.course, 0.0),
        Vec3::new(speed * radians.sin(), 0.0, speed * radians.cos()),
        date_time,
        false,
        VecDeque::new(),
    )
}
