use crate::gsm_time_converter::GsmTime;
use crate::sim800_parser::{
    ParsedUrc, classify_urc, line_indicates_call_connected, line_indicates_call_failed,
    parse_cclk_line, parse_clip_number, parse_clts_query_line, parse_cpbr_number,
    parse_dtmf_char, parse_sms_header_number,
};

#[test]
fn parses_cclk_time() {
    assert_eq!(
        parse_cclk_line(r#"+CCLK: "26/01/09,23:15:31+12""#),
        Some(GsmTime {
            year: 26,
            month: 1,
            day: 9,
            hour: 23,
            minute: 15,
            second: 31,
        })
    );
}

#[test]
fn rejects_invalid_cclk_time() {
    assert_eq!(parse_cclk_line(r#"+CCLK: "26/AA/09,23:15:31+12""#), None);
    assert_eq!(parse_cclk_line("+CCLK: missing"), None);
}

#[test]
fn parses_phone_numbers_from_modem_lines() {
    assert_eq!(parse_cpbr_number(r#"+CPBR: 2,"+12345",129,"A""#).as_deref(), Some("+12345"));
    assert_eq!(parse_sms_header_number(r#"+CMT: "+998","","""#).as_deref(), Some("+998"));
    assert_eq!(parse_clip_number(r#"+CLIP: "+777",145"#).as_deref(), Some("+777"));
}

#[test]
fn parses_dtmf_and_clts_lines() {
    assert_eq!(parse_dtmf_char("+DTMF: #"), Some('#'));
    assert_eq!(parse_clts_query_line("+CLTS: 1"), Some(true));
    assert_eq!(parse_clts_query_line("+CLTS: 0"), Some(false));
}

#[test]
fn classifies_urcs() {
    assert_eq!(
        classify_urc(r#"+CLIP: "+777",145"#),
        Some(ParsedUrc::Clip {
            number: "+777".try_into().unwrap(),
        })
    );
    assert_eq!(classify_urc("+DTMF: *"), Some(ParsedUrc::Dtmf('*')));
}

#[test]
fn detects_call_status_lines() {
    assert!(line_indicates_call_connected("+DTMF: *", '*'));
    assert!(line_indicates_call_connected("*", '*'));
    assert!(line_indicates_call_failed("NO CARRIER"));
    assert!(line_indicates_call_failed("BUSY"));
    assert!(!line_indicates_call_failed("OK"));
}
