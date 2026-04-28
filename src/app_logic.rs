use heapless::String;

use crate::alarms_handler::{AlarmStack, AlarmTracker};
use crate::constants::{
    ALARMS_MESSAGE_STRING_LENGTH, ALIVE_PERIOD_MINUTES, CALLBACK_PERIOD_MINUTES,
    DTMF_PACKET_LENGTH, SIM800_LINE_BUFFER_SIZE, SMS_DIVIDER, SMS_PREFIX,
};
use crate::custom_strings::extract_before_delimiter;
use crate::date_converter::format_gsm_time;
use crate::gsm_time_converter::GsmTime;
use heapless::String as HeaplessString;

#[derive(Clone, PartialEq, Debug)]
pub enum LogicCommand {
    HandleIncomingCall {
        phone_number: HeaplessString<{ crate::constants::MAX_PHONE_LENGTH }>,
    },
    SendAlarmSms {
        message: HeaplessString<SIM800_LINE_BUFFER_SIZE>,
    },
    CallAlarmWithDtmf {
        dtmf: HeaplessString<DTMF_PACKET_LENGTH>,
    },
}

#[derive(Clone, PartialEq, Debug)]
pub enum LogicEvent {
    SmsReceived {
        number: HeaplessString<{ crate::constants::MAX_PHONE_LENGTH }>,
        message: HeaplessString<SIM800_LINE_BUFFER_SIZE>,
    },
    CallReceived {
        number: HeaplessString<{ crate::constants::MAX_PHONE_LENGTH }>,
    },
    DtmfReceived(char),
    CallEnded,
    CallExecuted(bool),
    TimeReceived(GsmTime),
}

const WATCHDOG_TIMEOUT_SECS: u64 = 255 * 60;

#[derive(Clone)]
pub struct LogicState {
    pub alarm_stack: AlarmStack,
    pub alive_countdown: i32,
    pub pending_dtmf: Option<String<DTMF_PACKET_LENGTH>>,
    pub retry_countdown: Option<u32>,
    pub pending_alive_message: bool,
}

impl LogicState {
    pub const fn new() -> Self {
        Self {
            alarm_stack: AlarmStack::new(),
            alive_countdown: 0,
            pending_dtmf: None,
            retry_countdown: None,
            pending_alive_message: false,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum LogicAction {
    Visualize(String<DTMF_PACKET_LENGTH>),
    SendCommand(LogicCommand),
    BlinkAlarm3,
    UpdateRtc(GsmTime),
    SetWatchdog(Option<u64>),
}

pub fn handle_event(
    state: &mut LogicState,
    dtmf_buffer: &mut String<DTMF_PACKET_LENGTH>,
    event: LogicEvent,
) -> heapless::Vec<LogicAction, 4> {
    let mut actions = heapless::Vec::new();

    match event {
        LogicEvent::SmsReceived { message, .. } => {
            if let Some(alarm_str) = extract_alarm_payload(&message) {
                let _ = actions.push(LogicAction::Visualize(alarm_str));
                let _ = actions.push(LogicAction::SetWatchdog(Some(WATCHDOG_TIMEOUT_SECS)));
            }
        }
        LogicEvent::DtmfReceived(c) => {
            if dtmf_buffer.push(c).is_ok() && dtmf_buffer.len() == DTMF_PACKET_LENGTH {
                let mut packet = String::<DTMF_PACKET_LENGTH>::new();
                let _ = packet.push_str(dtmf_buffer.as_str());
                let _ = actions.push(LogicAction::Visualize(packet));
                let _ = actions.push(LogicAction::SetWatchdog(Some(WATCHDOG_TIMEOUT_SECS)));
                dtmf_buffer.clear();
            }
        }
        LogicEvent::CallEnded => {
            dtmf_buffer.clear();
        }
        LogicEvent::CallReceived { number } => {
            let _ = actions.push(LogicAction::SendCommand(LogicCommand::HandleIncomingCall {
                phone_number: number,
            }));
        }
        LogicEvent::CallExecuted(success) => {
            if success {
                state.alarm_stack.acknowledge_export();
                state.pending_dtmf = None;
                state.retry_countdown = None;
                state.pending_alive_message = false;
                let _ = actions.push(LogicAction::BlinkAlarm3);
            } else if state.pending_dtmf.is_some() {
                state.retry_countdown = Some(CALLBACK_PERIOD_MINUTES);
            }
        }
        LogicEvent::TimeReceived(time) => {
            let _ = actions.push(LogicAction::UpdateRtc(time));
        }
    }

    actions
}

pub fn handle_sender_tick(
    state: &mut LogicState,
    use_sms: bool,
    current_time: Option<&GsmTime>,
) -> heapless::Vec<LogicAction, 2> {
    let mut actions = heapless::Vec::new();
    let tick = state.alive_countdown <= 0;

    if let Some(countdown) = state.retry_countdown.as_mut() {
        if *countdown > 0 {
            *countdown -= 1;
        }
    }

    let should_retry = matches!(state.retry_countdown, Some(0));
    let should_send_new = state.pending_dtmf.is_none()
        && !state.pending_alive_message
        && (state.alarm_stack.has_changes() || tick);

    if should_retry {
        if let Some(dtmf) = state.pending_dtmf.clone() {
            state.retry_countdown = Some(CALLBACK_PERIOD_MINUTES);
            let _ = actions.push(LogicAction::SendCommand(LogicCommand::CallAlarmWithDtmf { dtmf }));
        } else {
            state.retry_countdown = None;
        }
    } else if should_send_new {
        let bits = state.alarm_stack.export_bits();
        let payload: String<DTMF_PACKET_LENGTH> = bits.iter().collect();
        state.alive_countdown = ALIVE_PERIOD_MINUTES + 1;

        if use_sms {
            if let Some(time) = current_time {
                let mut msg = String::<SIM800_LINE_BUFFER_SIZE>::new();
                let time_buf = format_gsm_time(time);
                use core::fmt::Write;
                let _ = write!(
                    msg,
                    "{}{}{}{}{}",
                    SMS_PREFIX,
                    SMS_DIVIDER,
                    payload,
                    SMS_DIVIDER,
                    time_buf.as_str()
                );
                let _ = actions.push(LogicAction::SendCommand(LogicCommand::SendAlarmSms {
                    message: msg,
                }));
            }
        } else {
            state.pending_dtmf = Some(payload.clone());
            state.retry_countdown = None;
            state.pending_alive_message = tick;
            let _ = actions.push(LogicAction::SendCommand(LogicCommand::CallAlarmWithDtmf {
                dtmf: payload,
            }));
        }
    }

    if !tick {
        state.alive_countdown -= 1;
    }

    actions
}

pub fn extract_alarm_payload(message: &str) -> Option<String<DTMF_PACKET_LENGTH>> {
    let alarm_str = extract_before_delimiter(message, ";")?;
    if alarm_str.len() != ALARMS_MESSAGE_STRING_LENGTH {
        return None;
    }

    let mut out = String::<DTMF_PACKET_LENGTH>::new();
    out.push_str(alarm_str).ok()?;
    Some(out)
}

pub fn watchdog_timeout_seconds() -> u64 {
    WATCHDOG_TIMEOUT_SECS
}
