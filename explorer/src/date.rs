use chrono::NaiveDate;
use js_sys::Date;
use std::convert::TryFrom;

pub enum DateConversionError {
    JsDateYearConversionError,
}

pub struct DateWrapper {
    pub date: Date
}

impl DateWrapper {
    pub fn new(date_obj: Date) -> DateWrapper {
        DateWrapper{
            date: date_obj
        }
    }
}

impl TryFrom<DateWrapper> for NaiveDate {
    type Error = DateConversionError;
    fn try_from(value: DateWrapper) -> Result<Self, Self::Error> {
        let date_year = if let Ok(i) = i32::try_from(value.date.get_full_year()) {
            i
        } else {
            return Err(DateConversionError::JsDateYearConversionError);
        };
        let date_month = value.date.get_month();
        let date_day = value.date.get_day();
        Ok(NaiveDate::from_ymd(date_year, date_month, date_day))
    }
}
