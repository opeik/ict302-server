use crate::types::*;

use libc::c_double;
use libloading::Symbol;

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

pub type PredictFn<'a> = Symbol<
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

pub async fn run_prediction(
    state: State,
    history: &Vec<f64>,
    num_predictions: usize,
) -> Result<(Vec<f64>, f64)> {
    let settings = Settings::new(1, -1, 15.0, 0);

    // Create the prediction result vector.
    let mut result = vec![0.0; num_predictions];
    // Create the FFI structs.
    let history = History::new(&history);
    let prediction = Prediction::new(&mut result);

    unsafe {
        let predict_fn: PredictFn = state.predictor.get(b"PredictTimeSeries")?;
        let error = predict_fn(&settings, &history, &prediction);

        Ok((result, error))
    }
}
