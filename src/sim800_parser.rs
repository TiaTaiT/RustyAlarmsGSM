use heapless::String;

use crate::constants::MAX_PHONE_LENGTH;
use crate::custom_strings::{extract_after_delimiter, extract_between_delimiters};
use crate::gsm_time_converter::GsmTime;

#[derive(Clone, PartialEq, Debug)]
pub enum ParsedUrc {
    SmsHeader { number: String<MAX_PHONE_LENGTH> },
    Clip { number: String<MAX_PHONE_LENGTH> },
    Dtmf(char),
    Time(GsmTime),
}

pub fn parse_cclk_line(line: &str) -> Option<GsmTime> {
    let content = extract_between_delimiters(line, "\"", "\"")?;
    let bytes = content.as_bytes();
    if bytes.len() < 17 {
        return None;
    }

    let parse2 = |i: usize| -> Option<u8> {
        let d1 = bytes.get(i)?.wrapping_sub(b'0');
        let d2 = bytes.get(i + 1)?.wrapping_sub(b'0');
        if d1 > 9 || d2 > 9 {
            return None;
        }
        Some(d1 * 10 + d2)
    };

    Some(GsmTime {
        year: parse2(0)?,
        month: parse2(3)?,
        day: parse2(6)?,
        hour: parse2(9)?,
        minute: parse2(12)?,
        second: parse2(15)?,
    })
}

pub fn parse_cpbr_number(line: &str) -> Option<String<MAX_PHONE_LENGTH>> {
    if !line.contains("+CPBR:") {
        return None;
    }
    parse_quoted_number(line)
}

pub fn parse_sms_header_number(line: &str) -> Option<String<MAX_PHONE_LENGTH>> {
    if !line.contains("+CMT:") {
        return None;
    }
    parse_quoted_number(line)
}

pub fn parse_clip_number(line: &str) -> Option<String<MAX_PHONE_LENGTH>> {
    if !line.contains("+CLIP:") {
        return None;
    }
    parse_quoted_number(line)
}

pub fn parse_dtmf_char(line: &str) -> Option<char> {
    let val = extract_after_delimiter(line, "+DTMF: ")?;
    val.trim().chars().next()
}

pub fn parse_clts_query_line(line: &str) -> Option<bool> {
    if !line.contains("+CLTS:") {
        return None;
    }

    let value = extract_after_delimiter(line, ":")?.trim();
    match value.chars().next()? {
        '1' => Some(true),
        '0' => Some(false),
        _ => None,
    }
}

pub fn line_indicates_call_connected(line: &str, online_signal: char) -> bool {
    line.contains(online_signal) || parse_dtmf_char(line) == Some(online_signal)
}

pub fn line_indicates_call_failed(line: &str) -> bool {
    line.contains("NO CARRIER") || line.contains("BUSY")
}

pub fn classify_urc(line: &str) -> Option<ParsedUrc> {
    if let Some(number) = parse_sms_header_number(line) {
        return Some(ParsedUrc::SmsHeader { number });
    }
    if let Some(number) = parse_clip_number(line) {
        return Some(ParsedUrc::Clip { number });
    }
    if let Some(c) = parse_dtmf_char(line) {
        return Some(ParsedUrc::Dtmf(c));
    }
    if let Some(time) = parse_cclk_line(line) {
        return Some(ParsedUrc::Time(time));
    }
    None
}

fn parse_quoted_number(line: &str) -> Option<String<MAX_PHONE_LENGTH>> {
    let number = extract_between_delimiters(line, "\"", "\"")?;
    let mut out = String::<MAX_PHONE_LENGTH>::new();
    out.push_str(number).ok()?;
    Some(out)
}
