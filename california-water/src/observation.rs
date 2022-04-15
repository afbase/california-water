use crate::reservoir::Reservoir;
use chrono::naive::NaiveDate;
use core::result::Result;
use csv::{ReaderBuilder, StringRecord};
use futures::future::{join, join_all};
use reqwest::Client;
use std::collections::BTreeMap;
const DATE_FORMAT: &str = "%Y%m%d %H%M";
const YEAR_FORMAT: &str = "%Y-%m-%d";
const CSV_ROW_LENGTH: usize = 9;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ObservationError {
    HttpRequestError,
    HttpResponseParseError,
    ObservationCollectionError,
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

#[derive(Debug, PartialEq, Clone)]
pub struct Observation {
    pub station_id: String,
    pub date_observation: NaiveDate,
    pub date_recording: NaiveDate,
    pub value: DataRecording,
}

impl Observation {
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
        let request_body =
            Observation::http_request_body(client, reservoir_id, start_date, end_date).await;
        if let Ok(body) = request_body {
            if let Ok(observations) = Observation::request_to_observations(body) {
                Ok(observations)
            } else {
                Err(ObservationError::HttpResponseParseError)
            }
        } else {
            Err(ObservationError::HttpRequestError)
        }
    }
    async fn http_request_body(
        client: &Client,
        reservoir_id: &str,
        start_date: &NaiveDate,
        end_date: &NaiveDate,
    ) -> Result<String, reqwest::Error> {
        let url = format!("http://cdec.water.ca.gov/dynamicapp/req/CSVDataServlet?Stations={}&SensorNums=15&dur_code=D&Start={}&End={}", reservoir_id, start_date.format(YEAR_FORMAT), end_date.format(YEAR_FORMAT));
        let response = client.get(url).send().await?;
        response.text().await
    }
    fn request_to_observations(request_body: String) -> Result<Vec<Observation>, ObservationError> {
        let mut rdr = ReaderBuilder::new()
            .has_headers(true)
            .from_reader(request_body.as_bytes());
        let records = rdr
            .records()
            .map(|x| x.expect("failed record parse").try_into())
            .collect::<Result<Vec<Observation>, _>>();
        if let Ok(recs) = records {
            Ok(recs)
        } else {
            Err(ObservationError::HttpResponseParseError)
        }
    }
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
        let data_value = match value.get(6).unwrap() {
            "BRT" => Ok(DataRecording::Brt),
            "ART" => Ok(DataRecording::Art),
            "---" => Ok(DataRecording::Dash),
            s => match s.parse::<u32>() {
                Err(_p) => Ok(DataRecording::Recording(0u32)),
                Ok(u) => Ok(DataRecording::Recording(u)),
            },
            _ => Err(()),
        };
        if duration == Ok(Duration::Daily) {
            return Ok(Observation {
                station_id: value.get(0).unwrap().to_string(),
                date_recording: date_recording_value.unwrap(),
                date_observation: date_observation_value.unwrap(),
                value: data_value.unwrap(),
            });
        }
        Err(())
    }
}

#[cfg(test)]
mod test {
    use super::DataRecording;
    use crate::observation::Observation;
    use chrono::NaiveDate;
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
            Observation::http_request_body(&client, reservoir_id, &start_date, &end_date).await;
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
}
