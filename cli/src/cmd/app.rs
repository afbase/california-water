use california_water::{observation::{Observation, ObservationError}, reservoir::Reservoir};
use chrono::{Local, NaiveDate};
use csv::{StringRecord, Writer};
use core::panic;
use std::{io::Write, path::Path};
// use futures::{future::join_all, stream};
use futures::{stream::{self, StreamExt}, future::join_all};
use reqwest::Client;

struct AppBuilder {
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub filetype: Option<FileType>,
    pub filename: Option<String>,
}
#[derive(Clone)]
enum FileType {
    PNG,
    CSV,
    STDOUT,
}
#[derive(Clone)]
struct App {
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub filetype: Option<FileType>,
    pub filename: Option<String>,
}

impl App {
    pub async fn run(self) {
        // 2. if csv or stdout run csv
        let fname = String::from(self.filename.unwrap().as_str());
        let app_copy = App {
            start_date: self.start_date.clone(),
            end_date: self.end_date.clone(),
            filetype: self.filetype.clone(),
            filename: Some(fname)
        };
        match app_copy.filetype.unwrap() {
            FileType::CSV => {
                let k = app_copy.filename.unwrap();
                let file_name = k.as_str();
                let p = Path::new(file_name);
                let csv_out = App::run_csv(&app_copy.start_date, &app_copy.end_date.unwrap()).await;
                let mut fs = std::fs::File::create(p).unwrap();
                fs.write_all(csv_out.as_bytes());
            },
            FileType::STDOUT => {
                let csv_out = App::run_csv(&app_copy.start_date, &app_copy.end_date.unwrap()).await;
                std::io::stdout().write_all(csv_out.as_bytes());
            },
            FileType::PNG => {
                // self.build_png().await;
            }
        }
    }

    async fn run_csv(start_date: &NaiveDate,
        end_date: &NaiveDate) -> String {
        // 1. get observations from date range
        let reservoirs = Reservoir::get_reservoir_vector();
        let client = Client::new();
        let all_reservoir_observations = join_all(reservoirs
            .iter()
            .map(|reservoir| {
            let client_ref = &client;
            let start_date_ref = start_date;
            let end_date_ref = end_date;
            async move {
                Observation::get_string_records(
                    client_ref,
                    reservoir.station_id.as_str(),
                    start_date_ref,
                    end_date_ref,
                )
                .await
            }
        }))
        .await;
        let mut writer = Writer::from_writer(vec![]);
        for reservoir_records in all_reservoir_observations {
            let records = reservoir_records.unwrap();
            // writer.write_byte_record(records.iter());
            for record in records {
                writer.write_byte_record(record.as_byte_record());
            }
        }
        String::from_utf8(writer.into_inner().unwrap()).unwrap()
    }
}

impl AppBuilder {
    // set app configuration
    pub fn new(start_date: NaiveDate) -> Self {
        Self {
            start_date,
            end_date: None,
            filetype: None,
            filename: None,
        }
    }

    pub fn end_date(&mut self, end_date: NaiveDate) -> &mut Self {
        self.end_date = Some(end_date);
        self
    }

    pub fn filetype(&mut self, filetype: FileType) -> &mut Self {
        self.filetype = Some(filetype);
        self
    }

    pub fn filename(&mut self, filename: String) -> &mut Self {
        self.filename = Some(filename);
        self
    }

    pub fn build(&mut self) -> App {
        // 1.0 check that end_date is more recent than start date, if exists
        // 1.1 if it doesn't exist, assume today's date.
        // 2. if filename is set, then filetype must be stated.
        // 2.1 if filetype is set, then file name must be stated.
        let mut app = App {
            start_date: self.start_date,
            end_date: None,
            filetype: None,
            filename: None,
        };
        // step 1.0
        if let Some(end_date) = self.end_date {
            if end_date <= self.start_date {
                panic!("Error: end date must be more recent than start date");
            }
            app.end_date = self.end_date;
        } else {
            // step 1.1
            let today = chrono::offset::Local::today().naive_local();
            if today < self.start_date {
                panic!("Error: start date must be not be in the future.  Either today or earlier");
            }
            app.end_date = Some(today);
        }
        // step 2
        let step_2_condition = self.filename.is_some() && self.filetype.is_none();
        if step_2_condition {
            panic!(
                "Error: filename set without filetype please specify filetype - either png, csv"
            );
        }
        // step 2.1
        let step_2_1_condition = self.filename.is_none() && self.filename.is_some();
        if step_2_1_condition {
            panic!("Error: filetype set without filename specified. please specify a filename");
        }
        if self.filename.is_some() {
            let k = self.filename.as_ref().unwrap();
            app.filename = Some(String::from(k.as_str()));
            app.filetype = Some(FileType::CSV);
        } else {
            app.filetype = Some(FileType::STDOUT);
        }
        app
    }
}
