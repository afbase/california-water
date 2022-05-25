pub mod cmd;
use clap::App;

use self::cmd::clap::new_app;
use self::cmd::app::AppBuilder;
use self::cmd::app::FileType;
use chrono::{NaiveDate, Utc};

#[tokio::main]
async fn main() -> Result<(), ()> {
    let matches = new_app().get_matches();
    let start_date = {
        let start = matches.value_of("start_date").expect("Needs a start date");
        NaiveDate::parse_from_str(start, "%Y%m%d").expect("start date format must be YYYMMDD")
    };
    let end_date = {
        let now = Utc::now().date().naive_local();
        if let Some(end) = matches.value_of("end_date") {
            NaiveDate::parse_from_str(end, "%Y%m%d").expect("end date needs to be YYYYMMDD format")
        } else {
            now
        }
    };
    let filetype = match matches.value_of("filetype") {
        Some("csv") => FileType::CSV,
        Some("png") => FileType::PNG,
        Some("stdout") => FileType::STDOUT,
        Some("lzma") => FileType::LZMA,
        _ => {panic!("filetype must be set to either csv, png, stdout")}
    };
    let output = match matches.value_of("output") {
        Some(value) => String::from(value),
        _ => String::new()
    };
    let app = AppBuilder::new(start_date)
    .end_date(end_date)
    .filetype(filetype)
    .filename(output)
    .build();
    app.run().await;
    Ok(())
}
