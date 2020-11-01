use crate::types::*;

use chrono::{DateTime, Duration, Utc};
use libc::c_double;
use libloading::{Library, Symbol};
use nalgebra_glm::Vec3;
use splines::{Interpolation, Key, Spline};

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

pub async fn predict_entity(
    state: State,
    entity_id: String,
    start: DateTime<Utc>,
    duration: Duration,
    spacing: Duration,
) -> Result<(Vec3, Vec3)> {
    const FACTOR: f64 = 100_000_000.0;

    let history = state.history.read().await;
    let entity_history = history.get(&entity_id).ok_or("Entity not found")?;

    let x_keys = entity_history
        .into_iter()
        .map(|e| {
            Key::new(
                e.date_time.timestamp() as f64,
                e.position.x as f64,
                Interpolation::Bezier(e.position.x as f64),
            )
        })
        .collect::<Vec<_>>();

    let z_keys = entity_history
        .into_iter()
        .map(|e| {
            Key::new(
                e.date_time.timestamp() as f64,
                e.position.z as f64,
                Interpolation::Bezier(e.position.z as f64),
            )
        })
        .collect::<Vec<_>>();

    let spline_x = Spline::from_vec(x_keys);
    let spline_z = Spline::from_vec(z_keys);

    let end_time = start.timestamp() as f64;
    let start_time = end_time - (duration.num_seconds() as f64);
    let num_minutes = (end_time - start_time) / (spacing.num_seconds() as f64);

    let mut history_x = vec![];
    let mut history_z = vec![];

    for i in 0..(num_minutes as i32) {
        let time = start_time + ((i as f64) * (spacing.num_seconds() as f64));

        history_x.push(spline_x.sample(time).unwrap() * FACTOR);
        history_z.push(spline_z.sample(time).unwrap() * FACTOR);
    }

    let (predictions_x, error_x) = run_prediction(history_x, 1)?;
    let (predictions_z, error_z) = run_prediction(history_z, 1)?;
    let x = predictions_x.get(0).unwrap() / FACTOR;
    let z = predictions_z.get(0).unwrap() / FACTOR;

    Ok((
        Vec3::new(x as f32, 0.0, z as f32),
        Vec3::new(error_x as f32, 0.0, error_z as f32),
    ))
}

pub fn run_prediction(history: Vec<f64>, num_predictions: usize) -> Result<(Vec<f64>, f64)> {
    let predictor = Library::new("predictor.module").expect("Failed to load predictor library");
    let settings_ffi = Settings::new(1, -1, 15.0, 0);

    // Create the prediction result vector.
    let mut result_ffi = vec![0.0; num_predictions];
    // Create the FFI structs.
    let history_ffi = History::new(&history);
    let prediction_ffi = Prediction::new(&mut result_ffi);

    unsafe {
        let predict_fn: PredictFn = predictor.get(b"PredictTimeSeries")?;
        let error = predict_fn(&settings_ffi, &history_ffi, &prediction_ffi);
        Ok((result_ffi, error))
    }
}
