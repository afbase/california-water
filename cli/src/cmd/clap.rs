use clap::{Arg, Command};

// /// Water Reservoir CLI Tool
// /// To Generate Data and Graphs
// #[derive(Parser, Debug)]
// #[clap(author, version, about, long_about=None)]
// pub(crate) struct Args {

// }

pub fn new_app() -> Command<'static> {
    Command::new("Water Reservoir CLI Tool")
        .version("")
        .author("Clinton Bowen <clinton.bowen@gmail.com>")
        .about("Graphs Water Table")
        .arg(
            Arg::new("start_date")
                .short('s')
                .long("start_date")
                .value_name("YYYYMMDD")
                .help("start date of graph")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::new("end_date")
                .short('e')
                .long("end_date")
                .value_name("YYYYMMDD")
                .help("end date of graph. If not supplied; today's date is assumed.")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::new("filetype")
                .short('t')
                .long("file type: png, csv")
                .help("png file name output")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("filename of output")
                .help("filename of output")
                .required(true)
                .takes_value(true),
        )
    // .arg(
    //     Arg::new("csv")
    //         .short("c")
    //         .long("csv")
    //         .required(true)
    //         .takes_value(true)
    //         .help("csv file of reservoir data"),
    // )
}
