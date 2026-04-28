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

#[test]
fn parse_gsm_time_accepts_mixed_delimiters_and_long_years() {
    let parsed = GsmTime {
        year: 0,
        month: 0,
        day: 0,
        hour: 0,
        minute: 0,
        second: 0,
    }
    .parse_gsm_time("2024/08/09,10:11:12+03");

    assert_eq!(
        parsed.map(|t| (t.year, t.month, t.day, t.hour, t.minute, t.second)),
        Some((24, 8, 9, 10, 11, 12))
    );
}

#[test]
fn parse_gsm_time_rejects_invalid_or_incomplete_input() {
    let parser = GsmTime {
        year: 0,
        month: 0,
        day: 0,
        hour: 0,
        minute: 0,
        second: 0,
    };

    assert!(parser.parse_gsm_time("24/13/09,10:11:12").is_none());
    assert!(parser.parse_gsm_time("24/08/09,10:11").is_none());
    assert!(parser.parse_gsm_time("not-a-date").is_none());
}