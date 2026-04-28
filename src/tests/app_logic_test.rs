use crate::alarms_handler::AlarmTracker;
use crate::app_logic::{LogicAction, LogicCommand, LogicEvent, LogicState, extract_alarm_payload, handle_event, handle_sender_tick};
use crate::gsm_time_converter::GsmTime;

#[test]
fn extracts_alarm_payload_only_for_valid_messages() {
    assert_eq!(extract_alarm_payload("1234;rest").as_deref(), Some("1234"));
    assert_eq!(extract_alarm_payload("123;rest"), None);
}

#[test]
fn dtmf_event_visualizes_full_packet_and_clears_buffer() {
    let mut state = LogicState::new();
    let mut buffer = heapless::String::new();

    for c in ['1', '2', '3'] {
        let actions = handle_event(&mut state, &mut buffer, LogicEvent::DtmfReceived(c));
        assert!(actions.is_empty());
    }

    let actions = handle_event(&mut state, &mut buffer, LogicEvent::DtmfReceived('4'));
    assert_eq!(buffer.len(), 0);
    assert_eq!(actions.len(), 2);
    assert!(matches!(&actions[0], LogicAction::Visualize(payload) if payload.as_str() == "1234"));
}

#[test]
fn call_received_event_requests_incoming_call_handling() {
    let mut state = LogicState::new();
    let mut buffer = heapless::String::new();
    let actions = handle_event(
        &mut state,
        &mut buffer,
        LogicEvent::CallReceived {
            number: "+998".try_into().unwrap(),
        },
    );

    assert!(matches!(
        &actions[0],
        LogicAction::SendCommand(LogicCommand::HandleIncomingCall { phone_number }) if phone_number.as_str() == "+998"
    ));
}

#[test]
fn successful_call_execution_clears_pending_state() {
    let mut state = LogicState::new();
    state.pending_dtmf = Some("1234".try_into().unwrap());
    state.pending_alive_message = true;
    state.retry_countdown = Some(1);
    state.logic_alarm_push();

    let mut buffer = heapless::String::new();
    let actions = handle_event(&mut state, &mut buffer, LogicEvent::CallExecuted(true));

    assert!(state.pending_dtmf.is_none());
    assert!(state.retry_countdown.is_none());
    assert!(!state.pending_alive_message);
    assert!(actions.iter().any(|action| matches!(action, LogicAction::BlinkAlarm3)));
}

#[test]
fn sender_tick_builds_alarm_sms_when_enabled() {
    let mut state = LogicState::new();
    state.logic_alarm_push();
    state.logic_alarm_push_variant();
    let time = GsmTime {
        year: 24,
        month: 12,
        day: 31,
        hour: 23,
        minute: 59,
        second: 58,
    };

    let actions = handle_sender_tick(&mut state, true, Some(&time));
    assert!(matches!(
        &actions[0],
        LogicAction::SendCommand(LogicCommand::SendAlarmSms { message }) if message.contains("PPP_")
    ));
}

#[test]
fn sender_tick_retries_pending_dtmf_call() {
    let mut state = LogicState::new();
    state.pending_dtmf = Some("9876".try_into().unwrap());
    state.retry_countdown = Some(0);

    let actions = handle_sender_tick(&mut state, false, None);
    assert_eq!(state.retry_countdown, Some(crate::constants::CALLBACK_PERIOD_MINUTES));
    assert!(matches!(
        &actions[0],
        LogicAction::SendCommand(LogicCommand::CallAlarmWithDtmf { dtmf }) if dtmf.as_str() == "9876"
    ));
}

trait LogicStateTestExt {
    fn logic_alarm_push(&mut self);
    fn logic_alarm_push_variant(&mut self);
}

impl LogicStateTestExt for LogicState {
    fn logic_alarm_push(&mut self) {
        self.alarm_stack.push(&[false, false, false, false]);
    }

    fn logic_alarm_push_variant(&mut self) {
        self.alarm_stack.push(&[true, false, false, false]);
    }
}
