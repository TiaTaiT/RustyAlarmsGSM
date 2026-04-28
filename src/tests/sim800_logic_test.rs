use crate::phone_book::PhoneBook;
use crate::sim800_logic::{
    AlarmCallDecision, AlarmDedupState, UsbCommandState, decide_alarm_call,
};

#[test]
fn suppresses_duplicate_alarm_calls_inside_window() {
    let mut dedup = AlarmDedupState::new();
    dedup.record_success("1234", 100);

    assert!(dedup.should_skip("1234", 150));
    assert!(!dedup.should_skip("1234", 230));
    assert!(!dedup.should_skip("9999", 150));
}

#[test]
fn decides_alarm_calls_from_phonebook_and_dedup_state() {
    let mut book = PhoneBook::new();
    book.add_number("+123456").unwrap();

    let dedup = AlarmDedupState::new();
    assert_eq!(
        decide_alarm_call(&book, &dedup, "1111", 10),
        AlarmCallDecision::PlaceCall {
            number: "+123456".try_into().unwrap(),
        }
    );

    let mut dedup = AlarmDedupState::new();
    dedup.record_success("1111", 50);
    assert_eq!(
        decide_alarm_call(&book, &dedup, "1111", 100),
        AlarmCallDecision::SkipDuplicate
    );

    let empty = PhoneBook::new();
    assert_eq!(
        decide_alarm_call(&empty, &dedup, "1111", 100),
        AlarmCallDecision::NoNumber
    );
}

#[test]
fn usb_command_state_executes_prefixed_commands() {
    let mut state = UsbCommandState::new();

    let first = state.push_byte(b'_', true);
    assert!(first.echo);
    assert!(!first.forward_to_modem);

    state.push_byte(b'a', true);
    let newline = state.push_byte(b'\n', true);
    assert_eq!(newline.command_to_execute.as_deref(), Some("_a"));
    assert!(newline.echo);
}

#[test]
fn usb_command_state_handles_backspace_and_passthrough() {
    let mut state = UsbCommandState::new();
    state.push_byte(b'_', false);
    state.push_byte(b'a', false);
    state.push_byte(0x08, false);
    let newline = state.push_byte(b'\r', false);
    assert_eq!(newline.command_to_execute.as_deref(), Some("_"));

    let passthrough = state.push_byte(b'A', true);
    assert!(passthrough.echo);
    assert!(passthrough.forward_to_modem);
}
