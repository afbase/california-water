use california_water::{observation::Observation, reservoir::Reservoir};
use chrono::NaiveDate;
use core::panic;
use csv::Writer;
use futures::future::join_all;
use lzma_rs::lzma_decompress;
use reqwest::Client;
use std::{
    io::{BufReader, Write},
    path::Path,
};
use tar::Archive;
pub struct AppBuilder {
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub filetype: Option<FileType>,
    pub filename: Option<String>,
    pub input_filename: Option<String>,
}

#[derive(Clone)]
pub enum FileType {
    PNG,
    CSV,
    STDOUT,
    LZMA,
}

#[derive(Clone)]
pub struct App {
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub filetype: Option<FileType>,
    pub filename: Option<String>,
    pub input_filename: Option<String>,
}

impl App {
    pub async fn run_decompress(self) {
        // 2. if csv or stdout run csv
        let fname = String::from(self.filename.unwrap().as_str());
        let input_fname = String::from(self.input_filename.unwrap().as_str());
        let app_copy = App {
            start_date: self.start_date,
            end_date: self.end_date,
            filetype: self.filetype,
            filename: Some(fname),
            input_filename: Some(input_fname),
        };
        match app_copy.filetype.unwrap() {
            FileType::LZMA => {
                let input_filename = app_copy.input_filename.unwrap();
                let output_filename = app_copy.filename.unwrap();
                let inp_fs = std::fs::File::open(input_filename).unwrap();
                let mut reader = BufReader::new(inp_fs);
                let mut input_bytes: Vec<u8> = Vec::new();
                if lzma_decompress(&mut reader, &mut input_bytes).is_err() {
                    panic!("decompression failed");
                }
                let mut arch = Archive::new(input_bytes.as_slice());
                if arch.unpack(output_filename).is_err() {
                    panic!("tar unpacking failed");
                }
            }
            _ => {
                panic!("needs to be a compression type");
            }
        }
    }
    pub async fn run(self) {
        // 2. if csv or stdout run csv
        let fname = String::from(self.filename.unwrap().as_str());
        let app_copy = App {
            start_date: self.start_date,
            end_date: self.end_date,
            filetype: self.filetype,
            filename: Some(fname),
            input_filename: None,
        };
        match app_copy.filetype.unwrap() {
            FileType::CSV => {
                let k = app_copy.filename.unwrap();
                let file_name = k.as_str();
                let p = Path::new(file_name);
                let csv_out = App::run_csv(&app_copy.start_date, &app_copy.end_date.unwrap()).await;
                let mut fs = std::fs::File::create(p).unwrap();
                if fs.write_all(csv_out.as_bytes()).is_err() {
                    panic!("writing csv file failed");
                }
            }
            FileType::STDOUT => {
                let csv_out = App::run_csv(&app_copy.start_date, &app_copy.end_date.unwrap()).await;
                if std::io::stdout().write_all(csv_out.as_bytes()).is_err() {
                    panic!("stdout failed");
                }
            }
            FileType::PNG => {
                // self.build_png().await;
            }
            _ => {
                panic!("error: needs to be either csv, stdout, or png");
            }
        }
    }

    async fn run_csv(start_date: &NaiveDate, end_date: &NaiveDate) -> String {
        // 1. get observations from date range
        let reservoirs = Reservoir::get_reservoir_vector();
        let client = Client::new();
        let all_reservoir_observations = join_all(reservoirs.iter().map(|reservoir| {
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
                if writer.write_byte_record(record.as_byte_record()).is_err() {
                    panic!("Error: writiing record failed");
                }
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
            input_filename: None,
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

    pub fn input_filename(&mut self, filename: String) -> &mut Self {
        self.input_filename = Some(filename);
        self
    }
    pub fn build_input_run(&mut self) -> App {
        let mut app = App {
            start_date: self.start_date,
            end_date: None,
            filetype: None,
            filename: None,
            input_filename: None,
        };

        if self.filename.is_none() {
            panic!("needs an output filename");
        }
        if self.input_filename.is_none() {
            panic!("needs an input filename");
        }
        if self.filetype.is_none() {
            panic!("needs a filetype for the input")
        }
        let k = self.filename.as_ref().unwrap();
        app.filename = Some(String::from(k.as_str()));
        let j = self.input_filename.as_ref().unwrap();
        app.input_filename = Some(String::from(j.as_str()));
        app.filetype = self.filetype.clone();
        app
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
            input_filename: None,
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
