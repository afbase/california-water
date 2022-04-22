// mod utils;
mod date;
use california_water::observation::Observation;
use chrono::{Datelike, NaiveDate};
use date::DateWrapper;
use easy_cast::Cast;
use js_sys::Date;
use plotters::prelude::*;
use plotters_canvas::CanvasBackend;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;
use std::convert::TryFrom;

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
    pub async fn build_chart(
        canvas: HtmlCanvasElement,
        start_date_js: Date,
        end_date_js: Date,
    ) -> Result<Chart, JsValue> {
        // get california water reservoir data
        let start_wrapper = DateWrapper::new(start_date_js);
        let end_wrapper = DateWrapper::new(end_date_js);
        let start_date = NaiveDate::try_from(start_wrapper).unwrap();
        let end_date = NaiveDate::try_from(end_wrapper).unwrap();
        if let Ok(data_water_btree) =
            Observation::get_all_reservoirs_data_by_dates(&start_date, &end_date).await
        {
            //Goal get max and min value of btree:
            let values = data_water_btree.values().cloned().collect::<Vec<u32>>();
            let y_max: f64 = (*values.iter().max().unwrap() as i64).cast();
            let y_min: f64 = (*values.iter().min().unwrap() as i64).cast();
            let x_max = values.len() as f64;
            let x_labels_amount = (end_date.year() - start_date.year()) as usize;
            // populate the canvas with the data
            let backend = CanvasBackend::with_canvas_object(canvas).unwrap();

            let root = backend.into_drawing_area();
            root.fill(&WHITE)?;

            let mut chart = ChartBuilder::on(&root)
                .margin(20)
                .x_label_area_size(10)
                .y_label_area_size(10)
                .build_cartesian_2d(start_date..end_date, y_min..y_max)?;
            // .build_cartesian_2d(-2.1..0.6, -1.2..1.2)?;

            chart
                .configure_mesh()
                .x_labels(x_labels_amount)
                // .disable_x_mesh()
                // .disable_y_mesh()
                .draw()?;
            chart
                .draw_series(LineSeries::new(
                    data_water_btree
                        .iter()
                        .map(|x| (*x.0, *x.1 as i32 as f64))
                        .collect::<Vec<_>>(),
                    &RED,
                ))
                .unwrap()
                .label("water")
                .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED));

            chart
                .configure_series_labels()
                .background_style(&WHITE.mix(0.8))
                .border_style(&BLACK)
                .draw()?;
            root.present()?;
            let boxed_chart_transform = Box::new(chart.into_coord_trans());
            Ok(Chart {
                convert: Box::new(move |coord| {
                    boxed_chart_transform(coord).map(|(x, y)| (x.into(), y.into()))
                }),
            })
        }
    }
}
