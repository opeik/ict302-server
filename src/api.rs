use crate::types::*;

use chrono::Duration;
use nalgebra_glm::{pi, Vec3};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;

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

    let mut entities = state.entities.write().await;

    response.vessels.into_iter().for_each(|v| {
        let entity_id = v.id.to_string();
        entities
            .entry(entity_id)
            .and_modify(|e| *e = new_entity_from_vessel(&v))
            .or_insert(new_entity_from_vessel(&v));
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

    Entity::new(
        Vec3::new(vessel.longitude, 0.0, vessel.latitude),
        Vec3::new(0.0, vessel.course, 0.0),
        Vec3::new(speed * radians.sin(), 0.0, speed * radians.cos()),
    )
}

use cgmath as cg;
use chrono::DateTime;
use splines::{Interpolation, Key, Spline};
use std::fs;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PredictionData {
    #[serde(rename = "type")]
    class: String,
    #[serde(rename = "subtype")]
    sub_class: String,
    data: Vec<cg::Vector3<f64>>,
}

pub async fn calc_training_data(time_span: Duration, spacing: Duration) -> Result<()> {
    let url = Url::parse_with_params(
        "http://localhost:5000",
        &[("timespan", time_span.num_minutes().to_string())],
    )?;

    let body = reqwest::get(url).await?.text().await?;
    let response: Response = serde_json::from_str(&body)?;
    assert_eq!(response.vessels.len(), response.result_length);

    // Map vessel IDs to vessels.
    let mut entity_map = HashMap::new();
    response.vessels.iter().for_each(|e| {
        entity_map
            .entry(e.id.to_string())
            .or_insert(vec![])
            .push(e.clone())
    });

    // Collect all known vessel positions.
    let mut keys = HashMap::new();
    entity_map.iter().for_each(|(k, v)| {
        v.iter().for_each(|e| {
            let timestamp = e.timestamp.to_owned() + " +0000";
            let dt = DateTime::parse_from_str(&timestamp, "%F %T %z").unwrap();

            keys.entry(k.clone()).or_insert(vec![]).push(Key::new(
                dt.timestamp() as f64,
                cg::Vector3::<f64>::new(e.longitude as f64, 0.0, e.latitude as f64),
                Interpolation::Bezier(cg::Vector3::new(e.longitude as f64, 0.0, e.latitude as f64)),
            ))
        })
    });

    // Perform the interpolation and collect the results.
    let mut data = HashMap::new();
    keys.iter().for_each(|(k, v)| {
        let spline = Spline::from_vec(v.to_vec());
        let start_time = keys.get(k).unwrap().first().unwrap().t;
        let end_time = keys.get(k).unwrap().last().unwrap().t;
        let num_minutes = (end_time - start_time) / 60.0;

        let entity = entity_map.get(k).unwrap().get(0).unwrap();

        for i in 0..(num_minutes as i32) {
            let time = start_time + ((i as f64) * (spacing.num_seconds() as f64));

            data.entry(k)
                .or_insert(PredictionData {
                    class: entity.class.to_owned(),
                    sub_class: entity.sub_class.to_owned(),
                    data: vec![],
                })
                .data
                .push(spline.sample(time).unwrap());
        }
    });

    let json = serde_json::to_string(&data)?;

    fs::write("data.json", json).expect("Unable to write file");
    info!("Done writing to disk.");

    Ok(())
}
