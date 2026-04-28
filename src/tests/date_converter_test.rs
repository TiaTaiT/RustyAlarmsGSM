// /src/tests/date_converter_test.rs
use crate::date_converter::format_gsm_time;
use crate::gsm_time_converter::GsmTime;

#[test]
fn test_format_gsm_time_produces_compact_timestamp() {
    let time = GsmTime {
        year: 24,
        month: 8,
        day: 9,
        hour: 10,
        minute: 11,
        second: 12,
    };

    assert_eq!(format_gsm_time(&time).as_str(), "240809101112");
}
