use crate::types::*;

use chrono::Duration;
use lazy_static::lazy_static;
use libc::c_double;
use libloading::{Library, Symbol};
use nalgebra_glm::Vec3;
use splines::{Interpolation, Key, Spline};
use tracing::debug;

lazy_static! {
    static ref PREDICTOR_LIB: Library =
        Library::new("predictor.module").expect("Failed to load predictor library");
    static ref PREDICTOR_FN: PredictFn<'static> =
        unsafe { PREDICTOR_LIB.get(b"PredictTimeSeries").unwrap() };
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Settings {
    pub lag: i32,
    pub seed: i32,
    pub stopping_criteria: c_double,
    pub log_results: u8,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct History {
    pub data: *const c_double,
    pub length: i32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Prediction {
    pub data: *mut c_double,
    pub length: i32,
}

type PredictFn<'a> = Symbol<
    'a,
    unsafe extern "C" fn(
        settings: *const Settings,
        history: *const History,
        result: *const Prediction,
    ) -> c_double,
>;

impl Settings {
    pub fn new(lag: i32, seed: i32, stopping_criteria: c_double, log_results: u8) -> Self {
        Self {
            lag,
            seed,
            stopping_criteria,
            log_results,
        }
    }
}

impl History {
    pub fn new(v: &Vec<f64>) -> Self {
        Self {
            data: v.as_ptr(),
            length: v.len() as i32,
        }
    }
}

impl Prediction {
    pub fn new(v: &mut Vec<f64>) -> Self {
        Self {
            data: v.as_mut_ptr(),
            length: v.len() as i32,
        }
    }
}

pub enum PredictionKind {
    #[allow(dead_code)]
    LinearExtrapolation,
    GeneticAlgorithm,
}

// Mitigates floating point errors.
const SCALE_FACTOR: f64 = 100_000_000.0;

pub fn predict_entity(
    state: State,
    entity_id: String,
    look_behind: Duration,
    spacing: Duration,
    num_predictions: usize,
    kind: PredictionKind,
) -> Result<(String, Vec<Vec3>, Vec3)> {
    match kind {
        PredictionKind::LinearExtrapolation => {
            let entities = futures::executor::block_on(state.entities.read());

            let entity = entities.get(&entity_id).ok_or("Entity not found")?;

            debug!("Predictions finished for entity {}", entity_id);

            let (predictions, error) = run_linear_extrapolation_prediction(
                entity.position,
                entity.angles,
                entity.velocity,
                num_predictions,
                spacing,
            )?;

            return Ok((entity_id.clone(), predictions, error));
        }
        PredictionKind::GeneticAlgorithm => {
            let history = futures::executor::block_on(state.history.read());
            let entity_history = history.get(&entity_id).ok_or("Entity not found")?;

            if entity_history.len() < 2 {
                return Err(Box::from("Not enough history"));
            }

            // Calculate the amount of predictions to make from the last update time.
            let last_update_time = entity_history.last().expect("No history found").date_time;
            let first_update_time = entity_history.first().expect("No history found").date_time;
            let history_duration = last_update_time - first_update_time;
            let min_history_points = look_behind.num_seconds() / spacing.num_seconds();

            assert!(history_duration > Duration::zero());

            // If we don't have enough data to use the genetic algorithm, just linearly extrapolate.
            if (history_duration.num_seconds() / spacing.num_seconds()) < min_history_points {
                let entities = futures::executor::block_on(state.entities.read());
                let entity = entities.get(&entity_id).ok_or("Entity not found")?;

                debug!("Predictions finished for entity {}", entity_id);

                let (predictions, error) = run_linear_extrapolation_prediction(
                    entity.position,
                    entity.angles,
                    entity.velocity,
                    num_predictions,
                    spacing,
                )?;

                return Ok((entity_id.clone(), predictions, error));
            }

            // Interpolate from the available history.
            let (history_x, _, history_z) = futures::executor::block_on(
                interpolate_entity_history(state.clone(), entity_id.clone(), look_behind, spacing),
            )?;

            let (predictions_x, error_x) =
                run_genetic_algorithm_prediction(history_x, num_predictions)?;
            let (predictions_z, error_z) =
                run_genetic_algorithm_prediction(history_z, num_predictions)?;

            let mut predictions = vec![];

            // Combine the axes.
            for i in 0..num_predictions {
                predictions.push(Vec3::new(
                    (predictions_x.get(i).unwrap() / SCALE_FACTOR) as f32,
                    0.0,
                    (predictions_z.get(i).unwrap() / SCALE_FACTOR) as f32,
                ));
            }

            debug!("Predictions finished for entity {}", entity_id);

            Ok((
                entity_id.clone(),
                predictions,
                Vec3::new(
                    (error_x / SCALE_FACTOR) as f32,
                    0.0,
                    (error_z / SCALE_FACTOR) as f32,
                ),
            ))
        }
    }
}

async fn interpolate_entity_history(
    state: State,
    entity_id: String,
    look_behind: Duration,
    spacing: Duration,
) -> Result<(Vec<f64>, Vec<f64>, Vec<f64>)> {
    let history = state.history.read().await;
    let entity_history = history.get(&entity_id).ok_or("Entity not found")?;

    assert!(entity_history.len() >= 2);
    let first_update_time = entity_history.first().expect("No history found").date_time;
    let last_update_time = entity_history.last().expect("No history found").date_time;
    let history_duration = last_update_time - first_update_time;

    // Collect each historical x position.
    let x_keys = entity_history
        .into_iter()
        .map(|e| {
            Key::new(
                e.date_time.timestamp() as f64,
                (e.position.x as f64) * SCALE_FACTOR,
                Interpolation::Bezier(e.position.x as f64),
            )
        })
        .collect::<Vec<_>>();

    // Collect each historical z position.
    let z_keys = entity_history
        .into_iter()
        .map(|e| {
            Key::new(
                e.date_time.timestamp() as f64,
                (e.position.z as f64) * SCALE_FACTOR,
                Interpolation::Bezier(e.position.z as f64),
            )
        })
        .collect::<Vec<_>>();

    // Create the splines.
    let spline_x = Spline::from_vec(x_keys);
    let spline_z = Spline::from_vec(z_keys);

    let end_time = last_update_time.timestamp() as f64;
    let start_time = if history_duration >= look_behind {
        end_time - (look_behind.num_seconds() as f64)
    } else {
        first_update_time.timestamp() as f64
    };
    let num_minutes = (end_time - start_time) / (spacing.num_seconds() as f64);

    let mut history_x = vec![];
    let mut history_z = vec![];

    // Perform the interpolation.
    for i in 0..(num_minutes as i32) {
        let time = start_time + ((i as f64) * (spacing.num_seconds() as f64));

        history_x.push(spline_x.sample(time).unwrap());
        history_z.push(spline_z.sample(time).unwrap());
    }

    Ok((history_x, vec![], history_z))
}

pub fn run_linear_extrapolation_prediction(
    position: Vec3,
    angles: Vec3,
    velocity: Vec3,
    num_predictions: usize,
    delta_time: Duration,
) -> Result<(Vec<Vec3>, Vec3)> {
    // Earth radius in meters.
    const EARTH_RADIUS: f32 = 63711000.0;

    assert!(delta_time > Duration::zero(), "Negative delta time");

    let mut predictions = vec![];

    for i in 0..num_predictions {
        let delta_time = (delta_time.num_seconds() as f32) * ((i as f32) + 1.0);
        let distance =
            ((velocity.x * delta_time).powf(2.0) + (velocity.z * delta_time).powf(2.0)).sqrt();

        let latitude = (position.z.to_radians().sin() * (distance / EARTH_RADIUS).cos()
            + position.z.to_radians().cos()
                * (distance / EARTH_RADIUS).sin()
                * angles.y.to_radians().cos())
        .asin()
        .to_degrees();

        let longitude = position.x.to_radians()
            + (angles.y.to_radians().sin()
                * (distance / EARTH_RADIUS).sin()
                * position.z.to_radians().cos())
            .atan2((distance / EARTH_RADIUS).cos() - position.z.to_radians().sin() * latitude.sin())
            .to_degrees();

        predictions.push(Vec3::new(longitude, 0.0, latitude));
    }

    Ok((predictions, Vec3::new(0.0, 0.0, 0.0)))
}

fn run_genetic_algorithm_prediction(
    history: Vec<f64>,
    num_predictions: usize,
) -> Result<(Vec<f64>, f64)> {
    const EXPECTED_HISTORY_LENGTH: usize = 24;
    assert!(history.len() == EXPECTED_HISTORY_LENGTH);
    assert!(num_predictions >= 1);

    let settings_ffi = Settings::new(12, 42069, 15.0, 0);

    // Create the prediction result vector.
    let mut result_ffi = vec![0.0; num_predictions];
    // Create the FFI structs.
    let history_ffi = History::new(&history);
    let prediction_ffi = Prediction::new(&mut result_ffi);

    unsafe {
        let error = PREDICTOR_FN(&settings_ffi, &history_ffi, &prediction_ffi);
        Ok((result_ffi, error))
    }
}
