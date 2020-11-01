use crate::types::*;

use libc::c_double;
use libloading::Symbol;
use tracing::info;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct PredictorSettings {
    pub lag: i32,
    pub seed: i32,
    pub stopping_criteria: c_double,
    pub log_results: u8,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct TimeSeriesData {
    pub data: *const c_double,
    pub length: i32,
}

pub type PredictFn<'a> = Symbol<
    'a,
    unsafe extern "C" fn(
        settings: *const PredictorSettings,
        history: *const TimeSeriesData,
        prediction: *mut TimeSeriesData,
    ) -> c_double,
>;

pub async fn run_prediction(state: State) -> Result<()> {
    let settings = PredictorSettings {
        lag: 1,
        seed: -1,
        stopping_criteria: 15.0,
        log_results: 0,
    };

    // Historic lat/long data
    let series = vec![0.0, 1.0, 2.0, 3.0, 4.0];
    // The prediction results
    let mut result = vec![0.0; 3];

    let history = TimeSeriesData {
        data: series.as_ptr(),
        length: series.len() as i32,
    };

    let mut prediction = TimeSeriesData {
        data: result.as_mut_ptr(),
        length: result.len() as i32,
    };

    unsafe {
        let predict_fn: PredictFn = state.predictor.get(b"PredictTimeSeries")?;
        let error = predict_fn(&settings, &history, &mut prediction);
        info!("Prediction: {:?}, error: {}", result, error);
        Ok(())
    }
}
