use crate::{
    compression::{decompress_tar_file_to_csv_string, TAR_OBJECT},
    reservoir::Reservoir,
};
use chrono::{format::format, naive::NaiveDate, Datelike};
use core::{panic, result::Result};
use csv::{ReaderBuilder, StringRecord, ByteRecord};
use futures::future::join_all;
use reqwest::Client;
use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap},
    str,
};
const DATE_FORMAT: &str = "%Y%m%d %H%M";
const YEAR_FORMAT: &str = "%Y-%m-%d";
const CSV_ROW_LENGTH: usize = 9;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ObservationError {
    HttpRequestError,
    HttpResponseParseError,
    ObservationCollectionError,
    FunctionFail,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Duration {
    Daily,
    Monthly,
}
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum DataRecording {
    Brt,
    Art,
    Dash,
    Recording(u32),
}

#[derive(Debug, Clone)]
pub struct Observation {
    pub station_id: String,
    pub date_observation: NaiveDate,
    pub date_recording: NaiveDate,
    pub value: DataRecording,
    pub duration: Duration,
}

impl Observation {
    // pub async fn get_all_reservoirs_csv_records_by_dates(
    //     start_date: &NaiveDate,
    //     end_date: &NaiveDate,
    // ) -> Result<BTreeMap<NaiveDate, u32>, ObservationError> {
    //     let reservoirs = Reservoir::get_reservoir_vector();
    //     let mut date_water_btree: BTreeMap<NaiveDate, u32> = BTreeMap::new();
    //     let client = Client::new();
    //     let all_reservoir_observations = join_all(reservoirs.iter().map(|reservoir| {
    //         let client_ref = &client;
    //         let start_date_ref = start_date;
    //         let end_date_ref = end_date;
    //         async move {
    //             Observation::get_string_records(
    //                 client_ref,
    //                 reservoir.station_id.as_str(),
    //                 start_date_ref,
    //                 end_date_ref,
    //             )
    //             .await
    //         }
    //     }))
    //     .await;
    // }
    pub fn get_all_records() -> Vec<StringRecord> {
        let bytes_of_csv_string = decompress_tar_file_to_csv_string(TAR_OBJECT);
        csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(bytes_of_csv_string.as_slice())
            .records()
            .map(|x| x.expect("failed record parse"))
            .collect::<Vec<StringRecord>>()
    }

    pub async fn get_all_reservoirs_data_by_dates(
        start_date: &NaiveDate,
        end_date: &NaiveDate,
    ) -> Result<BTreeMap<NaiveDate, u32>, ObservationError> {
        let reservoirs = Reservoir::get_reservoir_vector();
        let mut date_water_btree: BTreeMap<NaiveDate, u32> = BTreeMap::new();
        let client = Client::new();
        let all_reservoir_observations = join_all(reservoirs.iter().map(|reservoir| {
            let client_ref = &client;
            let start_date_ref = start_date;
            let end_date_ref = end_date;
            async move {
                Observation::get_observations(
                    client_ref,
                    reservoir.station_id.as_str(),
                    start_date_ref,
                    end_date_ref,
                )
                .await
            }
        }))
        .await;
        for reservoir_observations in all_reservoir_observations {
            let observations = reservoir_observations.unwrap();
            for observation in observations {
                let k = {
                    if let DataRecording::Recording(v) = observation.value {
                        v
                    } else {
                        0u32
                    }
                };
                date_water_btree
                    .entry(observation.date_observation)
                    .and_modify(|e| *e += k)
                    .or_insert(k);
            }
        }
        Ok(date_water_btree)
    }

    pub async fn get_observations(
        client: &Client,
        reservoir_id: &str,
        start_date: &NaiveDate,
        end_date: &NaiveDate,
    ) -> Result<Vec<Observation>, ObservationError> {
        let mut result: Result<Vec<Observation>, ObservationError> =
            Err(ObservationError::FunctionFail);
        let mut observations: Vec<Observation> = Vec::new();
        let request_body_daily =
            Observation::http_request_body(client, reservoir_id, start_date, end_date, "D").await;
        let request_body_monthly =
            Observation::http_request_body(client, reservoir_id, start_date, end_date, "M").await;
        if let Ok(body) = request_body_daily {
            if let Ok(mut daily_observations) = Observation::request_to_observations(body) {
                observations.append(&mut daily_observations);
            } else {
                result = Err(ObservationError::HttpResponseParseError);
            }
        } else {
            result = Err(ObservationError::HttpRequestError);
        }
        // collect monthly data and then
        // 1. linearly interpolate to daily observations
        // 2. insert into observations if the date does not exist
        if let Ok(body) = request_body_monthly {
            if let Ok(mut monthly_observations) = Observation::request_to_observations(body) {
                let mut observations_to_add_from_monthly_interpolations: Vec<Observation> =
                    Vec::new();
                // interpolate
                let daily_observations_from_monthly_observations_interpolated: Vec<Observation> =
                    Observation::linearly_interpolate_monthly_observations(
                        &mut monthly_observations,
                    );
                for interpolated_observation in
                    daily_observations_from_monthly_observations_interpolated
                {
                    let has_daily_value_is_recorded = observations.iter().any(|observation| {
                        let has_observation = interpolated_observation.date_observation
                            == observation.date_observation;
                        let is_recording =
                            matches!(observation.value, DataRecording::Recording(..));
                        has_observation && is_recording
                    });
                    if !has_daily_value_is_recorded {
                        observations_to_add_from_monthly_interpolations
                            .push(interpolated_observation);
                    }
                }
                observations.append(&mut observations_to_add_from_monthly_interpolations);
            } else {
                result = Err(ObservationError::HttpResponseParseError);
            }
        } else {
            result = Err(ObservationError::HttpRequestError);
        }
        result = Ok(observations);
        result
    }

    fn linearly_interpolate_monthly_observations(
        monthly_observations: &mut Vec<Observation>,
    ) -> Vec<Observation> {
        if monthly_observations.is_empty() {
            return Vec::new();
        }
        monthly_observations.sort();
        let mut output_vector: Vec<Observation> = Vec::new();
        // 1. need to make sure we have a pair of values we can operate over.
        let group_len = monthly_observations.len();
        let mut markers: Vec<usize> = Vec::new();
        let mut i: usize = 0;
        // for the ith and (i+1)th element,
        // 1. if ith element is a value and
        //    (i+1)th is not, mark i
        // 2. if not then ith element is
        //    some error.  if (i+1)th
        //    element is a value, then mark
        //    (i+1)
        loop {
            let observation = &monthly_observations[i];
            let next_observation = &monthly_observations[i + 1];
            match (observation.value, next_observation.value) {
                (DataRecording::Recording(..), DataRecording::Dash) => {
                    markers.push(i);
                    let monthly_recording_as_daily = Observation {
                        station_id: observation.station_id.clone(),
                        date_observation: observation.date_observation,
                        date_recording: observation.date_recording,
                        value: observation.value,
                        duration: Duration::Daily,
                    };
                    output_vector.push(monthly_recording_as_daily);
                }
                (DataRecording::Recording(..), DataRecording::Art) => {
                    markers.push(i);
                    let monthly_recording_as_daily = Observation {
                        station_id: observation.station_id.clone(),
                        date_observation: observation.date_observation,
                        date_recording: observation.date_recording,
                        value: observation.value,
                        duration: Duration::Daily,
                    };
                    output_vector.push(monthly_recording_as_daily);
                }
                (DataRecording::Recording(..), DataRecording::Brt) => {
                    markers.push(i);
                    let monthly_recording_as_daily = Observation {
                        station_id: observation.station_id.clone(),
                        date_observation: observation.date_observation,
                        date_recording: observation.date_recording,
                        value: observation.value,
                        duration: Duration::Daily,
                    };
                    output_vector.push(monthly_recording_as_daily);
                }
                (DataRecording::Dash, DataRecording::Recording(..)) => {
                    markers.push(i + 1);
                    let monthly_recording_as_daily = Observation {
                        station_id: next_observation.station_id.clone(),
                        date_observation: next_observation.date_observation,
                        date_recording: next_observation.date_recording,
                        value: next_observation.value,
                        duration: Duration::Daily,
                    };
                    output_vector.push(monthly_recording_as_daily);
                }
                (DataRecording::Art, DataRecording::Recording(..)) => {
                    markers.push(i + 1);
                    let monthly_recording_as_daily = Observation {
                        station_id: next_observation.station_id.clone(),
                        date_observation: next_observation.date_observation,
                        date_recording: next_observation.date_recording,
                        value: next_observation.value,
                        duration: Duration::Daily,
                    };
                    output_vector.push(monthly_recording_as_daily);
                }
                (DataRecording::Brt, DataRecording::Recording(..)) => {
                    markers.push(i + 1);
                    let monthly_recording_as_daily = Observation {
                        station_id: next_observation.station_id.clone(),
                        date_observation: next_observation.date_observation,
                        date_recording: next_observation.date_recording,
                        value: next_observation.value,
                        duration: Duration::Daily,
                    };
                    output_vector.push(monthly_recording_as_daily);
                }
                (DataRecording::Recording(..), DataRecording::Recording(..)) => {
                    markers.push(i);
                    markers.push(i + 1);
                }
                _ => {}
            }
            if i == (group_len - 2) {
                break;
            }
            i += 1; // do not i+2; still need to loop one-by-one
        }
        // for each array chunk pair[1]:
        // 1. do linear interpolation
        // 2. if markers is odd length, then
        // 2.1 from markers[len-1] to last observation:
        // 2.1.1 check there are no recordings, if so,
        // 2.1.2 set all recordings from markers[len-1]+1 to last observation
        //       to the value observation[markers[len-1]]
        // [1] - https://play.rust-lang.org/?version=nightly&mode=debug&edition=2018&gist=75bb6330866854040404a619c09c04f7
        let markers_slice = markers.as_slice();
        for [x0usize, x1usize] in markers_slice.array_chunks::<2>() {
            let x0 = *x0usize as u32;
            let date_x0 = monthly_observations[*x0usize].date_observation;
            let date_x1 = monthly_observations[*x1usize].date_observation;
            let delta_x = (date_x1 - date_x0).num_days().abs() as usize;
            let y0 = match monthly_observations[*x0usize].value {
                DataRecording::Recording(k) => k,
                _ => panic!("failed to select value"),
            };
            let y1 = match monthly_observations[*x1usize].value {
                DataRecording::Recording(k) => k,
                _ => panic!("failed to select value"),
            };
            let a = y1 - y0;
            let m = (a as f64) / (delta_x as f64);
            // just fill in entries between
            for xi in 1..delta_x {
                let y_i = (m * ((xi - (x0 as usize)) as f64) + (y0 as f64)).round() as u32;
                //make a daily observation
                let idx_duration = chrono::Duration::days(xi as i64);
                let date_observation =
                    monthly_observations[*x0usize].date_observation + idx_duration;
                let date_recording = monthly_observations[*x0usize].date_recording + idx_duration;
                let station_id = monthly_observations[*x0usize].station_id.clone();
                let ith_day_observation = Observation {
                    duration: Duration::Daily,
                    value: DataRecording::Recording(y_i),
                    date_observation,
                    date_recording,
                    station_id,
                };
                output_vector.push(ith_day_observation);
            }
            // add monthly_observations[*x0usize] and monthly_observations[*x1usize]
            // as daily observations
            let mut interpolated_thingers = vec![
                Observation {
                    duration: Duration::Daily,
                    value: monthly_observations[*x0usize].value,
                    date_observation: monthly_observations[*x0usize].date_observation,
                    date_recording: monthly_observations[*x0usize].date_recording,
                    station_id: monthly_observations[*x0usize].station_id.clone(),
                },
                Observation {
                    duration: Duration::Daily,
                    value: monthly_observations[*x1usize].value,
                    date_observation: monthly_observations[*x1usize].date_observation,
                    date_recording: monthly_observations[*x1usize].date_recording,
                    station_id: monthly_observations[*x1usize].station_id.clone(),
                },
            ];
            output_vector.append(&mut interpolated_thingers);
        }
        output_vector.sort();
        output_vector.dedup();
        output_vector
    }

    pub async fn get_string_records(
        client: &Client,
        reservoir_id: &str,
        start_date: &NaiveDate,
        end_date: &NaiveDate,
    ) -> Result<Vec<StringRecord>, ObservationError> {
        match Observation::get_observations(client, reservoir_id, start_date, end_date).await {
            Ok(observations) => {
                let mut ans: Vec<StringRecord> = Vec::with_capacity(observations.len());
                for obs in observations {
                    if let Ok(record) = obs.try_into() {
                        ans.push(record);
                    } else {
                        panic!("failed to convert record");
                    }
                }
                Ok(ans)
            },
            Err(e) => Err(e)
        }

    }
    async fn http_request_body(
        client: &Client,
        reservoir_id: &str,
        start_date: &NaiveDate,
        end_date: &NaiveDate,
        duration: &str,
    ) -> Result<String, reqwest::Error> {
        let url = format!("http://cdec.water.ca.gov/dynamicapp/req/CSVDataServlet?Stations={}&SensorNums=15&dur_code={}&Start={}&End={}", reservoir_id, duration, start_date.format(YEAR_FORMAT), end_date.format(YEAR_FORMAT));
        let response = client.get(url).send().await?;
        response.text().await
    }
    pub fn records_to_observations(vec_records: Vec<StringRecord>) -> Vec<Observation> {
        vec_records
            .iter()
            .map(|x| {
                let y = x.clone();
                y.try_into()
            })
            .collect::<Result<Vec<Observation>, _>>()
            .unwrap()
    }
    fn request_to_observations(request_body: String) -> Result<Vec<Observation>, ObservationError> {
        let string_records = Observation::request_to_string_records(request_body);
        let result = string_records
            .unwrap()
            .iter()
            .map(|x| {
                let y = x.clone();
                y.try_into()
            })
            .collect::<Result<Vec<Observation>, _>>();
        if let Ok(records) = result {
            Ok(records)
        } else {
            Err(ObservationError::ObservationCollectionError)
        }
    }
    fn request_to_string_records(
        request_body: String,
    ) -> Result<Vec<StringRecord>, ObservationError> {
        let records = ReaderBuilder::new()
            .has_headers(true)
            .from_reader(request_body.as_bytes())
            .records()
            .map(|x| x.expect("failed record parse"))
            .collect::<Vec<StringRecord>>();
        Ok(records)
    }
    /// Suppose we have gaps in our observations, e.g.:
    ///
    /// SHA,D,15,STORAGE,19850101 0000,19850101 0000,1543200,,AF
    /// SHA,D,15,STORAGE,19850102 0000,19850102 0000,---,,AF
    /// SHA,D,15,STORAGE,19850103 0000,19850103 0000,---,,AF
    /// SHA,D,15,STORAGE,19850104 0000,19850104 0000,---,,AF
    /// SHA,D,15,STORAGE,19850105 0000,19850105 0000,---,,AF
    /// SHA,D,15,STORAGE,19850106 0000,19850106 0000,1694200,,AF
    ///
    /// `smooth_observations` does a linear interpolation of the
    /// missing observations.
    ///
    /// From the example above, it becomes:
    /// SHA,D,15,STORAGE,19850101 0000,19850101 0000,1543200,,AF
    /// SHA,D,15,STORAGE,19850102 0000,19850102 0000,1573400,,AF
    /// SHA,D,15,STORAGE,19850103 0000,19850103 0000,1603600,,AF
    /// SHA,D,15,STORAGE,19850104 0000,19850104 0000,1633800,,AF
    /// SHA,D,15,STORAGE,19850105 0000,19850105 0000,1664000,,AF
    /// SHA,D,15,STORAGE,19850106 0000,19850106 0000,1694200,,AF
    pub fn smooth_observations(vec_records: &mut Vec<Observation>) -> Vec<Observation> {
        let mut output_vector: Vec<Observation> = Vec::with_capacity(vec_records.len());
        let observations_grouped_by_station_id = vec_records
            .as_slice()
            .group_by(|a, b| a.station_id == b.station_id);
        // this for loop does two things:
        // 1. Smoothy smoothy things by reservoir
        // 2. places smoothed observations by reservoir into output_vector
        for group in observations_grouped_by_station_id {
            let mut sorted_group = Vec::from(group);
            // sorting is the key step into the next flow
            sorted_group.sort();
            let sorted_group_len = sorted_group.len();
            let mut markers: Vec<usize> = Vec::new();
            let mut i: usize = 0;
            // for the ith and (i+1)th element,
            // 1. if ith element is a value and
            //    (i+1)th is not, mark i
            // 2. if not then ith element is
            //    some error.  if (i+1)th
            //    element is a value, then mark
            //    (i+1)
            loop {
                let observation = &sorted_group[i];
                let next_observation = &sorted_group[i + 1];
                match (observation.value, next_observation.value) {
                    (DataRecording::Recording(..), DataRecording::Dash) => {
                        markers.push(i);
                    }
                    (DataRecording::Recording(..), DataRecording::Art) => {
                        markers.push(i);
                    }
                    (DataRecording::Recording(..), DataRecording::Brt) => {
                        markers.push(i);
                    }
                    (DataRecording::Dash, DataRecording::Recording(..)) => {
                        markers.push(i + 1);
                    }
                    (DataRecording::Art, DataRecording::Recording(..)) => {
                        markers.push(i + 1);
                    }
                    (DataRecording::Brt, DataRecording::Recording(..)) => {
                        markers.push(i + 1);
                    }
                    _ => {}
                }
                if i == (sorted_group_len - 2) {
                    println!("catching the break");
                    break;
                }
                i += 1; // do not i+2; still need to loop one-by-one
            }
            // for each array chunk pair[1]:
            // 1. do linear interpolation
            // 2. if markers is odd length, then
            // 2.1 from markers[len-1] to last observation:
            // 2.1.1 check there are no recordings, if so,
            // 2.1.2 set all recordings from markers[len-1]+1 to last observation
            //       to the value observation[markers[len-1]]
            // [1] - https://play.rust-lang.org/?version=nightly&mode=debug&edition=2018&gist=75bb6330866854040404a619c09c04f7
            let markers_slice = markers.as_slice();
            for [x0usize, x1usize] in markers_slice.array_chunks::<2>() {
                let x0 = *x0usize as u32;
                let x1 = *x1usize as u32;
                let y0 = match sorted_group[*x0usize].value {
                    DataRecording::Recording(k) => k,
                    _ => panic!("failed to select value"),
                };
                let y1 = match sorted_group[*x1usize].value {
                    DataRecording::Recording(k) => k,
                    _ => panic!("failed to select value"),
                };
                let a = y1 - y0;
                let b = x1 - x0;
                let m = (a as f64) / (b as f64);

                for x_i in x0..x1 {
                    if x_i == x0 {
                        continue;
                    }
                    let y_i = (m * ((x_i - x0) as f64) + (y0 as f64)).round() as u32;
                    let x_i_as_usize = x_i as usize;
                    sorted_group[x_i_as_usize].value = DataRecording::Recording(y_i);
                }
            } // step 1
              // step 2
            let markers_len = markers.len();
            if markers_len % 2 == 1 {
                let mut xi = markers[markers_len - 1];
                // step 2.1.1
                let mut is_need_of_filling = true;
                loop {
                    if let DataRecording::Recording(..) = sorted_group[xi].value {
                        is_need_of_filling = false;
                        break;
                    }
                    if xi == sorted_group_len {
                        break;
                    }
                    xi += 1;
                }
                // step 2.1.2
                if is_need_of_filling {
                    let k = sorted_group[markers[markers_len - 1]].value;
                    for item in sorted_group
                        .iter_mut()
                        .take(sorted_group_len)
                        .skip(markers[markers_len - 1] + 1)
                    {
                        item.value = k;
                    }
                    // for idx in (markers[markers_len-1] + 1)..group_len {
                    //     sorted_group[idx].value = k;
                    // }
                }
            }
            output_vector.append(&mut sorted_group);
        }
        output_vector
    }

    pub fn vector_to_hashmap(
        vec_observations: Vec<Observation>,
    ) -> HashMap<String, Vec<Observation>> {
        let mut result: HashMap<String, Vec<Observation>> = HashMap::new();
        let groups = vec_observations
            .as_slice()
            .group_by(|a, b| a.station_id == b.station_id);
        for reservoir_observations in groups {
            let reservoir_id = &reservoir_observations[0].station_id;
            result.insert(reservoir_id.clone(), Vec::from(reservoir_observations));
        }
        result
    }
}

impl TryFrom<Observation> for StringRecord {
    fn try_from(value: Observation) -> Result<Self, Self::Error> {
        //         r#"STATION_ID,DURATION,SENSOR_NUMBER,SENSOR_TYPE,DATE TIME,OBS DATE,VALUE,DATA_FLAG,UNITS
        // VIL,D,15,STORAGE,20220215 0000,20220215 0000,9593, ,AF";
        let station_id = value.station_id.to_uppercase();
        let station_id_str = station_id.as_str();
        let duration = match value.duration {
            Duration::Daily => "D",
            Duration::Monthly => "M",
        };
        let sensor_number = "15";
        let sensor_type = "STORAGE";
        let date_time = format!(
            "{}{:02}{:02} 0000",
            value.date_recording.year(),
            value.date_recording.month(),
            value.date_recording.day()
        );
        let date_time_str = date_time.as_str();
        let date_obs = format!(
            "{}{:02}{:02} 0000",
            value.date_observation.year(),
            value.date_observation.month(),
            value.date_observation.day()
        );
        let date_obs_str = date_obs.as_str();
        let val = match value.value {
            DataRecording::Recording(a) => a.to_string(),
            DataRecording::Art => String::from("ART"),
            DataRecording::Brt => String::from("BRT"),
            DataRecording::Dash => String::from("---"),
        };
        let val_str = val.as_str();
        let data_flag = "";
        let units = "AF";
        let b = ByteRecord::from(vec![
            station_id_str,
            duration,
            sensor_number,
            sensor_type,
            date_time_str,
            date_obs_str,
            val_str,
            data_flag,
            units,
        ]);
        match StringRecord::from_byte_record(b) {
            Ok(s) => Ok(s),
            Err(_) => Err(()),
        }
    }

    type Error = ();
}

impl TryFrom<StringRecord> for Observation {
    type Error = ();

    fn try_from(value: StringRecord) -> Result<Self, Self::Error> {
        if value.len() != CSV_ROW_LENGTH {
            return Err(());
        }
        let duration = match value.get(1).unwrap() {
            "D" => Ok(Duration::Daily),
            "M" => Ok(Duration::Monthly),
            _ => Err(()),
        };
        let date_recording_value = NaiveDate::parse_from_str(value.get(4).unwrap(), DATE_FORMAT);
        let date_observation_value = NaiveDate::parse_from_str(value.get(5).unwrap(), DATE_FORMAT);
        let data_value: Result<DataRecording, ()> = match value.get(6).unwrap() {
            "BRT" => Ok(DataRecording::Brt),
            "ART" => Ok(DataRecording::Art),
            "---" => Ok(DataRecording::Dash),
            s => match s.parse::<u32>() {
                Err(_p) => Ok(DataRecording::Recording(0u32)),
                Ok(u) => Ok(DataRecording::Recording(u)),
            },
            // _ => Err(()),
        };
        if let Ok(..) = duration {
            return Ok(Observation {
                station_id: value.get(0).unwrap().to_string(),
                date_recording: date_recording_value.unwrap(),
                date_observation: date_observation_value.unwrap(),
                value: data_value.unwrap(),
                duration: duration.unwrap(),
            });
        }
        Err(())
    }
}

impl Ord for Observation {
    fn cmp(&self, other: &Self) -> Ordering {
        self.date_observation.cmp(&other.date_observation)
    }
}
impl Eq for Observation {}

impl PartialEq for Observation {
    fn eq(&self, other: &Self) -> bool {
        self.date_observation == other.date_observation
            && self.station_id == other.station_id
            && self.date_recording == other.date_recording
            && self.value == other.value
    }
}

impl PartialOrd for Observation {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.station_id != other.station_id {
            return None;
        }
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod test {
    use super::{DataRecording, Duration};
    use crate::observation::Observation;
    use chrono::NaiveDate;
    use csv::StringRecord;
    use reqwest::Client;
    use std::assert_ne;

    // https://cdec.water.ca.gov/dynamicapp/req/CSVDataServlet?Stations=VIL&SensorNums=15&dur_code=D&Start=2022-02-15&End=2022-02-28
    const STR_RESULT: &str = r#"STATION_ID,DURATION,SENSOR_NUMBER,SENSOR_TYPE,DATE TIME,OBS DATE,VALUE,DATA_FLAG,UNITS
VIL,D,15,STORAGE,20220215 0000,20220215 0000,9593, ,AF
VIL,D,15,STORAGE,20220216 0000,20220216 0000,9589, ,AF
VIL,D,15,STORAGE,20220217 0000,20220217 0000,9589, ,AF
VIL,D,15,STORAGE,20220218 0000,20220218 0000,9585, ,AF
VIL,D,15,STORAGE,20220219 0000,20220219 0000,9585, ,AF
VIL,D,15,STORAGE,20220220 0000,20220220 0000,9585, ,AF
VIL,D,15,STORAGE,20220221 0000,20220221 0000,9581, ,AF
VIL,D,15,STORAGE,20220222 0000,20220222 0000,9593, ,AF
VIL,D,15,STORAGE,20220223 0000,20220223 0000,9601, ,AF
VIL,D,15,STORAGE,20220224 0000,20220224 0000,9601, ,AF
VIL,D,15,STORAGE,20220225 0000,20220225 0000,9601, ,AF
VIL,D,15,STORAGE,20220226 0000,20220226 0000,9597, ,AF
VIL,D,15,STORAGE,20220227 0000,20220227 0000,9597, ,AF
VIL,D,15,STORAGE,20220228 0000,20220228 0000,9597, ,AF
"#;

    #[cfg(not(target_family = "wasm"))]
    #[tokio::test]
    async fn test_get_all_reservoirs_data_by_dates() {
        let start_date = NaiveDate::from_ymd(2022, 02, 15);
        let end_date = NaiveDate::from_ymd(2022, 02, 28);
        let obs = Observation::get_all_reservoirs_data_by_dates(&start_date, &end_date)
            .await
            .unwrap();
        for (_, val) in obs.iter() {
            assert_ne!(*val, 0u32);
        }
    }
    #[cfg(not(target_family = "wasm"))]
    #[tokio::test]
    async fn test_http_request_body() {
        // ID , DAM , LAKE          , STREAM        , CAPACITY (AF), YEAR FILL
        // VIL, Vail, Vail Reservoir, Temecula Creek, 51000,
        // https://cdec.water.ca.gov/dynamicapp/req/CSVDataServlet?Stations=VIL&SensorNums=15&dur_code=D&Start=2022-02-15&End=2022-02-28
        let reservoir_id = "VIL";
        let start_date = NaiveDate::from_ymd(2022, 02, 15);
        let end_date = NaiveDate::from_ymd(2022, 02, 28);
        let client = Client::new();
        let observations =
            Observation::http_request_body(&client, reservoir_id, &start_date, &end_date, "D")
                .await;
        assert_eq!(
            observations.unwrap().as_str().replace("\r\n", "\n"),
            STR_RESULT
        );
    }

    #[cfg(not(target_family = "wasm"))]
    #[tokio::test]
    async fn test_get_observations() {
        // ID , DAM , LAKE          , STREAM        , CAPACITY (AF), YEAR FILL
        // VIL, Vail, Vail Reservoir, Temecula Creek, 51000,
        // https://cdec.water.ca.gov/dynamicapp/req/CSVDataServlet?Stations=VIL&SensorNums=15&dur_code=D&Start=2022-02-15&End=2022-02-28
        let reservoir_id = "VIL";
        let start_date = NaiveDate::from_ymd(2022, 02, 15);
        let end_date = NaiveDate::from_ymd(2022, 02, 28);
        let client = Client::new();
        let observations =
            Observation::get_observations(&client, reservoir_id, &start_date, &end_date).await;
        assert_eq!(observations.unwrap().len(), 14);
    }

    #[test]
    fn test_request_to_observations() {
        // ID , DAM , LAKE          , STREAM        , CAPACITY (AF), YEAR FILL
        // VIL, Vail, Vail Reservoir, Temecula Creek, 51000,
        // https://cdec.water.ca.gov/dynamicapp/req/CSVDataServlet?Stations=VIL&SensorNums=15&dur_code=D&Start=2022-02-15&End=2022-02-28
        let string_result = String::from(STR_RESULT);
        let observations = Observation::request_to_observations(string_result).unwrap();
        assert_eq!(observations[0].value, DataRecording::Recording(9593));
    }

    #[test]
    fn test_smooth_observations() {
        /// SHA,D,15,STORAGE,19850101 0000,19850101 0000,1543200,,AF
        /// SHA,D,15,STORAGE,19850102 0000,19850102 0000,1573400,,AF
        /// SHA,D,15,STORAGE,19850103 0000,19850103 0000,1603600,,AF
        /// SHA,D,15,STORAGE,19850104 0000,19850104 0000,1633800,,AF
        /// SHA,D,15,STORAGE,19850105 0000,19850105 0000,1664000,,AF
        /// SHA,D,15,STORAGE,19850106 0000,19850106 0000,1694200,,AF
        let expected_observations = vec![
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 01),
                date_recording: NaiveDate::from_ymd(1985, 01, 01),
                value: DataRecording::Recording(1543200),
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 02),
                date_recording: NaiveDate::from_ymd(1985, 01, 02),
                value: DataRecording::Recording(1573400),
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 03),
                date_recording: NaiveDate::from_ymd(1985, 01, 03),
                value: DataRecording::Recording(1603600),
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 04),
                date_recording: NaiveDate::from_ymd(1985, 01, 04),
                value: DataRecording::Recording(1633800),
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 05),
                date_recording: NaiveDate::from_ymd(1985, 01, 05),
                value: DataRecording::Recording(1664000),
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 06),
                date_recording: NaiveDate::from_ymd(1985, 01, 06),
                value: DataRecording::Recording(1694200),
                duration: Duration::Daily,
            },
        ];
        let mut test_sample = vec![
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 01),
                date_recording: NaiveDate::from_ymd(1985, 01, 01),
                value: DataRecording::Recording(1543200),
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 02),
                date_recording: NaiveDate::from_ymd(1985, 01, 02),
                value: DataRecording::Dash,
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 03),
                date_recording: NaiveDate::from_ymd(1985, 01, 03),
                value: DataRecording::Dash,
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 04),
                date_recording: NaiveDate::from_ymd(1985, 01, 04),
                value: DataRecording::Art,
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 05),
                date_recording: NaiveDate::from_ymd(1985, 01, 05),
                value: DataRecording::Brt,
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 06),
                date_recording: NaiveDate::from_ymd(1985, 01, 06),
                value: DataRecording::Recording(1694200),
                duration: Duration::Daily,
            },
        ];
        let smooth_operator = Observation::smooth_observations(&mut test_sample);
        assert_eq!(
            smooth_operator, expected_observations,
            "failed to smooth observations"
        )
    }

    #[test]
    fn test_linearly_interpolate_observations() {
        /// SHA,D,15,STORAGE,19850101 0000,19850101 0000,1543200,,AF
        /// SHA,D,15,STORAGE,19850102 0000,19850102 0000,1573400,,AF
        /// SHA,D,15,STORAGE,19850103 0000,19850103 0000,1603600,,AF
        /// SHA,D,15,STORAGE,19850104 0000,19850104 0000,1633800,,AF
        /// SHA,D,15,STORAGE,19850105 0000,19850105 0000,1664000,,AF
        /// SHA,D,15,STORAGE,19850106 0000,19850106 0000,1694200,,AF
        let expected_observations = vec![
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 01),
                date_recording: NaiveDate::from_ymd(1985, 01, 01),
                value: DataRecording::Recording(1543200),
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 02),
                date_recording: NaiveDate::from_ymd(1985, 01, 02),
                value: DataRecording::Recording(1573400),
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 03),
                date_recording: NaiveDate::from_ymd(1985, 01, 03),
                value: DataRecording::Recording(1603600),
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 04),
                date_recording: NaiveDate::from_ymd(1985, 01, 04),
                value: DataRecording::Recording(1633800),
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 05),
                date_recording: NaiveDate::from_ymd(1985, 01, 05),
                value: DataRecording::Recording(1664000),
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 06),
                date_recording: NaiveDate::from_ymd(1985, 01, 06),
                value: DataRecording::Recording(1694200),
                duration: Duration::Daily,
            },
        ];
        let mut test_sample = vec![
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 01),
                date_recording: NaiveDate::from_ymd(1985, 01, 01),
                value: DataRecording::Recording(1543200),
                duration: Duration::Monthly,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 06),
                date_recording: NaiveDate::from_ymd(1985, 01, 06),
                value: DataRecording::Recording(1694200),
                duration: Duration::Monthly,
            },
        ];
        let smooth_operator =
            Observation::linearly_interpolate_monthly_observations(&mut test_sample);
        assert_eq!(
            smooth_operator, expected_observations,
            "failed to smooth observations"
        )
    }

    #[test]
    fn test_linearly_interpolate_observations2() {
        /// SHA,D,15,STORAGE,19850101 0000,19850101 0000,1543200,,AF
        /// SHA,D,15,STORAGE,19850102 0000,19850102 0000,1573400,,AF
        /// SHA,D,15,STORAGE,19850103 0000,19850103 0000,1603600,,AF
        /// SHA,D,15,STORAGE,19850104 0000,19850104 0000,1633800,,AF
        /// SHA,D,15,STORAGE,19850105 0000,19850105 0000,1664000,,AF
        /// SHA,D,15,STORAGE,19850106 0000,19850106 0000,1694200,,AF
        let expected_observations = vec![
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 01),
                date_recording: NaiveDate::from_ymd(1985, 01, 01),
                value: DataRecording::Recording(1543200),
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 02),
                date_recording: NaiveDate::from_ymd(1985, 01, 02),
                value: DataRecording::Recording(1573400),
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 03),
                date_recording: NaiveDate::from_ymd(1985, 01, 03),
                value: DataRecording::Recording(1603600),
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 04),
                date_recording: NaiveDate::from_ymd(1985, 01, 04),
                value: DataRecording::Recording(1633800),
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 05),
                date_recording: NaiveDate::from_ymd(1985, 01, 05),
                value: DataRecording::Recording(1664000),
                duration: Duration::Daily,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 06),
                date_recording: NaiveDate::from_ymd(1985, 01, 06),
                value: DataRecording::Recording(1694200),
                duration: Duration::Daily,
            },
        ];
        let mut test_sample = vec![
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 01),
                date_recording: NaiveDate::from_ymd(1985, 01, 01),
                value: DataRecording::Recording(1543200),
                duration: Duration::Monthly,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 03),
                date_recording: NaiveDate::from_ymd(1985, 01, 03),
                value: DataRecording::Dash,
                duration: Duration::Monthly,
            },
            Observation {
                station_id: String::from("SHA"),
                date_observation: NaiveDate::from_ymd(1985, 01, 06),
                date_recording: NaiveDate::from_ymd(1985, 01, 06),
                value: DataRecording::Recording(1694200),
                duration: Duration::Monthly,
            },
        ];
        let smooth_operator =
            Observation::linearly_interpolate_monthly_observations(&mut test_sample);
        assert_eq!(
            smooth_operator, expected_observations,
            "failed to smooth observations"
        )
    }
    #[test]
    fn test_observation_to_stringrecord() {
        /// SHA,D,15,STORAGE,19850106 0000,19850106 0000,1694200,,AF
        let obs_daily = Observation {
            station_id: String::from("SHA"),
            date_observation: NaiveDate::from_ymd(1985, 01, 06),
            date_recording: NaiveDate::from_ymd(1985, 01, 06),
            value: DataRecording::Recording(1694200),
            duration: Duration::Daily,
        };
        let obs_monthly = Observation {
            station_id: String::from("SHA"),
            date_observation: NaiveDate::from_ymd(1985, 01, 06),
            date_recording: NaiveDate::from_ymd(1985, 01, 06),
            value: DataRecording::Recording(1694200),
            duration: Duration::Monthly,
        };
        let obs_daily_string_record: StringRecord = obs_daily.try_into().unwrap();
        let obs_monthly_string_record: StringRecord = obs_monthly.try_into().unwrap();
        assert_eq!(&obs_daily_string_record[0], "SHA");
        assert_eq!(&obs_daily_string_record[1], "D");
        assert_eq!(&obs_daily_string_record[2], "15");
        assert_eq!(&obs_daily_string_record[3], "STORAGE");
        assert_eq!(&obs_daily_string_record[4], "19850106 0000");
        assert_eq!(&obs_daily_string_record[5], "19850106 0000");
        assert_eq!(&obs_daily_string_record[6], "1694200");
        assert_eq!(&obs_daily_string_record[7], "");
        assert_eq!(&obs_daily_string_record[8], "AF");
        assert_eq!(&obs_monthly_string_record[1], "M");

    }
}
