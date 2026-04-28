use heapless::Vec;

#[derive(Debug, Clone, Copy, defmt::Format, PartialEq)]
pub struct GsmTime {
    pub year: u8,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl GsmTime {
    fn parse_u8(s: &[u8]) -> Option<u8> {
        let mut result = 0u8;
        for &byte in s {
            if byte < b'0' || byte > b'9' {
                return None;
            }
            let digit = byte - b'0';
            result = result.checked_mul(10)?.checked_add(digit)?;
        }
        Some(result)
    }

    fn parse_u16_to_u8_year(s: &[u8]) -> Option<u8> {
        let mut result = 0u16;
        for &byte in s {
            if byte < b'0' || byte > b'9' {
                return None;
            }
            let digit = (byte - b'0') as u16;
            result = result.checked_mul(10)?.checked_add(digit)?;
        }
        Some((result % 100) as u8)
    }

    pub fn parse_gsm_time(&self, date: &str) -> Option<GsmTime> {
        // Normalize the input: keep only digits, replace everything else with ','
        let mut result_buf = [0u8; 64]; // larger buffer to be safe
        let mut result_len = 0usize;

        for &byte in date.as_bytes() {
            if result_len >= result_buf.len() {
                break;
            }
            if byte.is_ascii_digit() {
                result_buf[result_len] = byte;
            } else {
                result_buf[result_len] = b',';
            }
            result_len += 1;
        }

        // Split by commas and collect non-empty parts
        let mut parts: heapless::Vec<&[u8], 8> = heapless::Vec::new();
        let mut start = 0;

        for i in 0..=result_len {
            if i == result_len || result_buf[i] == b',' {
                if start < i {
                    let part = &result_buf[start..i];
                    if !part.is_empty() {
                        let _ = parts.push(part);
                    }
                }
                start = i + 1;
            }
        }

        // We need at least 6 numeric parts (year, month, day, hour, min, sec)
        // Extra parts (e.g. timezone) are ignored
        if parts.len() < 6 {
            return None;
        }

        // Parse year: support 2-digit or 4-digit (take last 2 digits if 4)
        let year = if parts[0].len() > 2 {
            Self::parse_u16_to_u8_year(parts[0])?
        } else {
            Self::parse_u8(parts[0])?
        };

        let month = Self::parse_u8(parts[1])?;
        let day = Self::parse_u8(parts[2])?;
        let hour = Self::parse_u8(parts[3])?;
        let minute = Self::parse_u8(parts[4])?;
        let second = Self::parse_u8(parts[5])?;

        // Basic range validation (note: day > 31 is rejected here, but real calendar validation is stricter)
        if !(1..=12).contains(&month)
            || !(1..=31).contains(&day)
            || hour > 23
            || minute > 59
            || second > 59
        {
            return None;
        }

        Some(GsmTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
        })
    }
}