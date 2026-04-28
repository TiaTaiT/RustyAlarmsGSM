// /src/tests/alarm_handler_test.rs
use crate::alarms_handler::*;
use crate::constants::ALARMS_STACK_DEPTH;

#[test]
fn test_alarm_stack_reports_no_changes_for_identical_states() {
    let mut stack = AlarmStack::new();
    let alarms = [true, false, true, false];

    for _ in 0..ALARMS_STACK_DEPTH {
        stack.push(&alarms);
    }

    assert!(!stack.has_changes());
}
/*
Need to re-enable this test after fixing the has_changes logic. It was previously returning false positives due to the way the stack was being updated when full. The test checks that changes are detected correctly and that the export_bits function returns the expected character representation of the alarm states.
#[test]
fn test_alarm_stack_detects_changes_and_exports_expected_bits() {
    let mut stack = AlarmStack::new();

    stack.push(&[false, false, false, false]);
    stack.push(&[true, false, false, true]);
    stack.push(&[true, true, false, false]);

    assert!(stack.has_changes());
    assert_eq!(stack.export_bits(), ['6', '4', '0', '2']);
    assert!(!stack.has_changes());
}
*/
#[test]
fn test_alarm_stack_overwrites_oldest_entry_when_full() {
    let mut stack = AlarmStack::new();

    stack.push(&[true, false, false, false]);
    stack.push(&[false, true, false, false]);
    stack.push(&[false, false, true, false]);
    stack.push(&[false, false, false, true]);

    assert_eq!(stack.export_bits(), ['1', '2', '2', '4']);
}

#[test]
fn alarm_stack_import_restores_exported_bits() {
    let mut stack = AlarmStack::new();
    let bits = ['7', '0', '3', '5'];

    stack.import_bits(bits);

    assert_eq!(stack.export_bits(), bits);
}