use js_sys::Date;
use chrono::NaiveDate;
use std::convert::TryFrom;

pub enum DateConversionError {
    JsDateYearConversionError
}

impl TryFrom<Date> for NaiveDate {
    type Error = DateConversionError;
    fn try_from(value: Date) -> Result<Self, Self::Error> {
        let date_year = match i32::try_from(date_js.get_full_year()) {
            Ok(i) => i,
            _ => {
                return Err(DateConversionError::JsDateYearConversionError)
            }
        };
        let date_month = date_js.get_month();
        let date_day = date_js.get_day();
        Ok(NaiveDate::from_ymd(date_year, date_month, date_day))
    }
}