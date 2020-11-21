use crate::types::*;

use chrono::{DateTime, Duration, Utc};
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

#[derive(Clone, Debug)]
pub enum PredictionKind {
    LinearExtrapolation,
    GeneticAlgorithm,
}

// Mitigates floating point errors.
const SCALE_FACTOR: f64 = 100_000_000.0;
const LINEAR_INTERPOLATION_ERROR: f32 = 10.0;

fn is_position_within_error_bounds(base: Vec3, error: Vec3, position: Vec3) -> bool {
    (position.x >= (base.x - error.x))
        && (position.x <= (base.x + error.x))
        && (position.z >= (base.z - error.z))
        && (position.z <= (base.z + error.z))
}

pub fn is_entity_in_anomalous_position(entity: Entity, history: Vec<Entity>) -> bool {
    if entity.prediction_cache.is_empty() {
        return false;
    }

    let mut is_in_anomalous_position = false;
    let prediction = entity
        .prediction_cache
        .front()
        .expect("Prediction cache empty");

    for i in history {
        if !is_position_within_error_bounds(entity.position, prediction.error, i.position) {
            is_in_anomalous_position = true;
        }
    }

    is_in_anomalous_position
}

pub fn interpolate_cached_prediction(
    prediction: crate::types::Prediction,
    spacing: Duration,
    current_position: Vec3,
    current_time: DateTime<Utc>,
) -> crate::types::Prediction {
    let start_time = prediction.date_time.timestamp();
    let end_time = (prediction.date_time + spacing).timestamp();

    let start_x = Key::new(start_time as f64, current_position.x, Interpolation::Linear);
    let end_x = Key::new(
        end_time as f64,
        prediction.position.x,
        Interpolation::default(),
    );
    let spline_x = Spline::from_vec(vec![start_x, end_x]);

    let start_z = Key::new(start_time as f64, current_position.z, Interpolation::Linear);
    let end_z = Key::new(
        end_time as f64,
        prediction.position.z,
        Interpolation::default(),
    );
    let spline_z = Spline::from_vec(vec![start_z, end_z]);

    crate::types::Prediction {
        position: Vec3::new(
            spline_x
                .clamped_sample(current_time.timestamp() as f64)
                .unwrap(),
            0.0,
            spline_z
                .clamped_sample(current_time.timestamp() as f64)
                .unwrap(),
        ),
        error: prediction.error,
        date_time: current_time,
        kind: PredictionKind::LinearExtrapolation,
    }
}

pub fn predict_entity(
    state: State,
    entity_id: String,
    look_behind: Duration,
    spacing: Duration,
    num_predictions: usize,
    kind: PredictionKind,
) -> Result<(String, Vec<crate::types::Prediction>)> {
    match kind {
        PredictionKind::LinearExtrapolation => {
            let entities = futures::executor::block_on(state.entities.read());
            let history = futures::executor::block_on(state.history.read());

            let entity = entities.get(&entity_id).ok_or("Entity not found")?;
            let entity_history = &history
                .get(&entity_id)
                .ok_or("Entity not found")?
                .history
                .clone();

            if entity_history.len() < 2 {
                return Err(Box::from("Not enough history"));
            }

            let current_time = chrono::offset::Utc::now();

            debug!("Predictions finished for entity {}", entity_id);

            let (predictions, error) = run_linear_extrapolation_prediction(
                entity.position,
                entity.angles,
                entity.velocity,
                num_predictions,
                spacing,
            )?;

            let combined_predictions = predictions
                .iter()
                .enumerate()
                .map(|(i, e)| crate::types::Prediction {
                    position: e.clone(),
                    error: error.clone(),
                    date_time: current_time + (spacing * (i as i32)),
                    kind: PredictionKind::LinearExtrapolation,
                })
                .collect();

            return Ok((entity_id.clone(), combined_predictions));
        }
        PredictionKind::GeneticAlgorithm => {
            let history = futures::executor::block_on(state.history.read());
            let entity_history = &history
                .get(&entity_id)
                .ok_or("Entity not found")?
                .history
                .clone();

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
                let current_time = chrono::offset::Utc::now();

                debug!("Predictions finished for entity {}", entity_id);

                let (predictions, error) = run_linear_extrapolation_prediction(
                    entity.position,
                    entity.angles,
                    entity.velocity,
                    num_predictions,
                    spacing,
                )?;

                let combined_predictions = predictions
                    .iter()
                    .enumerate()
                    .map(|(i, e)| crate::types::Prediction {
                        position: e.clone(),
                        error: error.clone(),
                        date_time: current_time + (spacing * (i as i32)),
                        kind: PredictionKind::GeneticAlgorithm,
                    })
                    .collect();

                return Ok((entity_id.clone(), combined_predictions));
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
            let current_time = chrono::offset::Utc::now();

            let combined_predictions = predictions
                .iter()
                .enumerate()
                .map(|(i, e)| crate::types::Prediction {
                    position: e.clone(),
                    error: Vec3::new(
                        (error_x / SCALE_FACTOR) as f32,
                        0.0,
                        (error_z / SCALE_FACTOR) as f32,
                    ),
                    date_time: current_time + (spacing * (i as i32)),
                    kind: PredictionKind::GeneticAlgorithm,
                })
                .collect();

            return Ok((entity_id.clone(), combined_predictions));
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
    let entity_history = &history
        .get(&entity_id)
        .ok_or("Entity not found")?
        .history
        .clone();

    assert!(entity_history.len() >= 2);
    let first_update_time = entity_history.first().expect("No history found").date_time;
    let last_update_time = entity_history.last().expect("No history found").date_time;
    let history_duration = last_update_time - first_update_time;

    // Collect each historical x position.
    let x_keys = entity_history
        .iter()
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
        .iter()
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
    spacing: Duration,
) -> Result<(Vec<Vec3>, Vec3)> {
    let mut predictions = vec![];

    for i in 0..num_predictions {
        let distance = ((velocity.x.powf(2.0) + velocity.z.powf(2.0)).sqrt())
            * (spacing.num_seconds() as f32 * (i as f32 + 1.0));
        let r: f32 = 63711000.0; // Earth Radius in m

        let lat2 = (position.z.to_radians().sin() * (distance / r).cos()
            + position.z.to_radians().cos() * (distance / r).sin() * angles.y.to_radians().cos())
        .asin();

        let lon2 = position.x.to_radians()
            + (angles.y.to_radians().sin() * (distance / r).sin() * position.z.to_radians().cos())
                .atan2((distance / r).cos() - position.z.to_radians().sin() * lat2.sin());

        let x = lon2.to_degrees();
        let z = lat2.to_degrees();

        predictions.push(Vec3::new(x, 0.0, z));
    }

    Ok((
        predictions,
        Vec3::new(LINEAR_INTERPOLATION_ERROR, 0.0, LINEAR_INTERPOLATION_ERROR),
    ))
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
