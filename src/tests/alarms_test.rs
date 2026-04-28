// /src/tests/alarms_test.rs
use crate::{alarms::AlarmStack, constants::{ALARMS_CHANNELS_AMOUNT, ALARMS_MESSAGE_STRING_LENGTH}};

#[test]
fn exported_alarm_bits_have_expected_shape() {
    let mut stack = AlarmStack::new();
    stack.push(&[true; ALARMS_CHANNELS_AMOUNT]);
    let bits = stack.export_bits();

    assert_eq!(bits.len(), ALARMS_MESSAGE_STRING_LENGTH);
}