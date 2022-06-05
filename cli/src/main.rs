pub mod cmd;
use clap::ArgMatches;

use self::cmd::app::AppBuilder;
use self::cmd::app::FileType;
use self::cmd::clap::new_app;
use chrono::{NaiveDate, Utc};

#[tokio::main]
async fn main() -> Result<(), ()> {
    match new_app().get_matches().subcommand() {
        Some(("decompress", app)) => decompress_run(app).await,
        Some(("data", app)) => data_run(app).await,
        _ => {
            panic!("needs to use subcommand")
        }
    }
}

async fn decompress_run(app: &ArgMatches) -> Result<(), ()> {
    let filetype = match app.value_of("filetype") {
        Some("csv") => FileType::CSV,
        Some("png") => FileType::PNG,
        Some("stdout") => FileType::STDOUT,
        Some("lzma") => FileType::LZMA,
        _ => {
            panic!("filetype must be set to either csv, png, stdout")
        }
    };
    let output = match app.value_of("output") {
        Some(value) => String::from(value),
        _ => String::new(),
    };
    let input = match app.value_of("input") {
        Some(value) => String::from(value),
        _ => String::new(),
    };
    let now = Utc::now().date().naive_local();
    let app = AppBuilder::new(now)
        .filetype(filetype)
        .filename(output)
        .input_filename(input)
        .build_input_run();
    app.run_decompress().await;
    Ok(())
}

async fn data_run(app: &ArgMatches) -> Result<(), ()> {
    let start_date = {
        let start = app.value_of("start_date").expect("Needs a start date");
        NaiveDate::parse_from_str(start, "%Y%m%d").expect("start date format must be YYYMMDD")
    };
    let end_date = {
        let now = Utc::now().date().naive_local();
        if let Some(end) = app.value_of("end_date") {
            NaiveDate::parse_from_str(end, "%Y%m%d").expect("end date needs to be YYYYMMDD format")
        } else {
            now
        }
    };
    let filetype = match app.value_of("filetype") {
        Some("csv") => FileType::CSV,
        Some("png") => FileType::PNG,
        Some("stdout") => FileType::STDOUT,
        Some("lzma") => FileType::LZMA,
        _ => {
            panic!("filetype must be set to either csv, png, stdout")
        }
    };
    let output = match app.value_of("output") {
        Some(value) => String::from(value),
        _ => String::new(),
    };
    let app = AppBuilder::new(start_date)
        .end_date(end_date)
        .filetype(filetype)
        .filename(output)
        .build();
    app.run().await;
    Ok(())
}
