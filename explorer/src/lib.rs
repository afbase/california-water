// mod utils;
mod date;
use futures::future::join_all;
use wasm_bindgen::prelude::*;
use web_sys::{HtmlCanvasElement, DateTimeValue};
use js_sys::Date;
use chrono::NaiveDate;
use california_water::{observation::Observation, reservoir::Reservoir};
use reqwest::Client;
// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

/// Type alias for the result of a drawing function.
pub type DrawResult<T> = Result<T, Box<dyn std::error::Error>>;

/// Type used on the JS side to convert screen coordinates to chart
/// coordinates.
#[wasm_bindgen]
pub struct Chart {
    convert: Box<dyn Fn((i32, i32)) -> Option<(f64, f64)>>,
}

/// Result of screen to chart coordinates conversion.
#[wasm_bindgen]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Chart {
    pub fn build_chart(canvas: HtmlCanvasElement, start_date_js: Date, end_date_js: Date ) -> Result<Chart, JsValue> {
        // get california water reservoir data
        let start_date = NaiveDate::try_from(start_date_js).unwrap();
        let end_date = NaiveDate::try_from(end_date_js).unwrap();
        let client = Client::new();
        let reservoirs = Reservoir::get_reservoir_vector();

        Observation::get_observations(&client, reservoir_id, &start_date, &end_date)
        // populate the canvas with the data
    }
}




