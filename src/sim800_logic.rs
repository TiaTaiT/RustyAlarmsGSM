use heapless::String;

use crate::constants::{DTMF_PACKET_LENGTH, MAX_PHONE_LENGTH};
use crate::phone_book::PhoneBook;

const DUPLICATE_ALARM_WINDOW_SEC: u64 = 120;

#[derive(Clone, PartialEq, Debug)]
pub enum AlarmCallDecision {
    NoNumber,
    SkipDuplicate,
    PlaceCall { number: String<MAX_PHONE_LENGTH> },
}

#[derive(Clone, PartialEq, Debug)]
pub struct AlarmDedupState {
    pub last_dtmf: String<DTMF_PACKET_LENGTH>,
    pub last_time: u64,
}

impl AlarmDedupState {
    pub fn new() -> Self {
        Self {
            last_dtmf: String::new(),
            last_time: 0,
        }
    }

    pub fn should_skip(&self, dtmf: &str, now: u64) -> bool {
        self.last_dtmf.as_str() == dtmf
            && now.saturating_sub(self.last_time) < DUPLICATE_ALARM_WINDOW_SEC
    }

    pub fn record_success(&mut self, dtmf: &str, now: u64) {
        self.last_dtmf.clear();
        let _ = self.last_dtmf.push_str(dtmf);
        self.last_time = now;
    }
}

impl Default for AlarmDedupState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn first_phonebook_number(book: &PhoneBook) -> Option<String<MAX_PHONE_LENGTH>> {
    let mut out = String::<MAX_PHONE_LENGTH>::new();
    out.push_str(book.get_first()?).ok()?;
    Some(out)
}

pub fn decide_alarm_call(
    book: &PhoneBook,
    dedup: &AlarmDedupState,
    dtmf: &str,
    now: u64,
) -> AlarmCallDecision {
    let Some(number) = first_phonebook_number(book) else {
        return AlarmCallDecision::NoNumber;
    };

    if dedup.should_skip(dtmf, now) {
        AlarmCallDecision::SkipDuplicate
    } else {
        AlarmCallDecision::PlaceCall { number }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct UsbCommandState {
    in_command_mode: bool,
    buf: String<64>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct UsbInputOutcome {
    pub echo: bool,
    pub forward_to_modem: bool,
    pub command_to_execute: Option<String<64>>,
}

impl UsbCommandState {
    pub fn new() -> Self {
        Self {
            in_command_mode: false,
            buf: String::new(),
        }
    }

    pub fn reset(&mut self) {
        self.in_command_mode = false;
        self.buf.clear();
    }

    pub fn push_byte(&mut self, byte: u8, modem_powered: bool) -> UsbInputOutcome {
        let c = byte as char;

        if !self.in_command_mode && c == '_' {
            self.in_command_mode = true;
            self.buf.clear();
            let _ = self.buf.push('_');
            return UsbInputOutcome {
                echo: true,
                forward_to_modem: false,
                command_to_execute: None,
            };
        }

        if self.in_command_mode {
            if c == '\r' || c == '\n' {
                let command_to_execute = if self.buf.is_empty() {
                    None
                } else {
                    Some(self.buf.clone())
                };
                self.buf.clear();
                self.in_command_mode = false;
                return UsbInputOutcome {
                    echo: true,
                    forward_to_modem: false,
                    command_to_execute,
                };
            }

            if c == '\x08' || c == '\x7f' {
                self.buf.pop();
            } else if self.buf.len() < self.buf.capacity() {
                let _ = self.buf.push(c);
            }

            return UsbInputOutcome {
                echo: true,
                forward_to_modem: false,
                command_to_execute: None,
            };
        }

        UsbInputOutcome {
            echo: true,
            forward_to_modem: modem_powered,
            command_to_execute: None,
        }
    }
}

impl Default for UsbCommandState {
    fn default() -> Self {
        Self::new()
    }
}
