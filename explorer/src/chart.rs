use std::collections::BTreeMap;
use california_water::observation::Observation;
use chrono::{Datelike, NaiveDate};
use crate::date::DateWrapper;
use easy_cast::Cast;
use js_sys::Date;
use plotters::prelude::*;
use plotters_canvas::CanvasBackend;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;
use std::convert::TryFrom;
/// Type alias for the result of a drawing function.
pub type DrawResult<T> = Result<T, Box<dyn std::error::Error>>;

/// Result of screen to chart coordinates conversion.
#[wasm_bindgen]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Type used on the JS side to convert screen coordinates to chart
/// coordinates.
#[wasm_bindgen]
pub struct Chart {
    convert: Box<dyn Fn((i32, i32)) -> Option<(f64, f64)>>,
}

struct ReservoirObservationChart {
    data_btree: BTreeMap<NaiveDate, u32>,
    canvas: HtmlCanvasElement
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
        let observations = Observation::get_all_reservoirs_data_by_dates(&start_date, &end_date)
        .await;
        // reservoir all the things
        let reservoir_chart = ReservoirObservationChart {
            data_btree: observations,
            canvas: canvas
        };
        reservoir_chart.chart()
    }

    /// This function can be used to convert screen coordinates to
    /// chart coordinates.
    pub fn coord(&self, x: i32, y: i32) -> Option<Point> {
        (self.convert)((x, y)).map(|(x, y)| Point { x, y })
    }
}

impl ReservoirObservationChart {
    fn chart(self: Self) -> Result<Chart, JsValue> {
        let dates = self.data_btree.keys().cloned().collect();
        let start_date = dates.nth(0);
        let end_date = dates.last();
        //Goal get max and min value of btree:
        let values = self.data_btree.values().cloned().collect::<Vec<u32>>();
        let y_max: f64 = (*values.iter().max().unwrap() as i64).cast();
        let y_min: f64 = (*values.iter().min().unwrap() as i64).cast();
        let x_max = values.len() as f64;
        let x_labels_amount = (end_date.year() - start_date.year()) as usize;
        // setup chart
        // setup canvas drawing area
        let backend = CanvasBackend::with_canvas_object(self.canvas).unwrap();
        let backend_drawing_area = backend.into_drawing_area();
        backend_drawing_area.fill(&WHITE);
        let mut chart = ChartBuilder::on(&backend_drawing_area)
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
        
        // populate the canvas with the data
        chart
            .draw_series(LineSeries::new(
                self.data_btree
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
        backend_drawing_area.present()?;
        let boxed_chart_transform = Box::new(chart.into_coord_trans())
        .map_err(|err| err.to_string())?;
        Ok(Chart {
            convert: Box::new(boxed_chart_transform),
        })
    }
}