use crate::app_logic::LogicState;
use crate::constants::{ALARMS_CHANNELS_AMOUNT, BATTERY_UNDERVOLTAGE_THRESHOLD, INTRUSION_CHANNELS_AMOUNT};
use crate::monitoring::{MonitorUpdate, SensorSnapshot, apply_monitor_update, evaluate_monitor_update};
use crate::system_state::SystemState;

#[test]
fn evaluate_monitor_update_detects_alarm_changes_and_tamper_edges() {
    let previous = SystemState {
        logic: LogicState::new(),
        battery_level: 3900,
        tamper_detected: false,
        adc_values: [0; INTRUSION_CHANNELS_AMOUNT],
        current_alarms: [false; ALARMS_CHANNELS_AMOUNT],
        power_connected: true,
    };

    let update = evaluate_monitor_update(
        &previous,
        SensorSnapshot {
            battery_level: BATTERY_UNDERVOLTAGE_THRESHOLD + 1,
            tamper_detected: true,
            power_connected: false,
            adc_values: [1500, 500],
        },
    );

    assert_eq!(update.current_alarms, [true, false, true, false]);
    assert!(update.alarms_changed);
    assert!(update.tamper_just_detected);
    assert_eq!(update.battery_level, BATTERY_UNDERVOLTAGE_THRESHOLD + 1);
    assert!(!update.power_connected);
}

#[test]
fn apply_monitor_update_replaces_runtime_fields_without_touching_logic_state() {
    let mut state = SystemState::new();
    state.logic.pending_alive_message = true;

    let update = MonitorUpdate {
        adc_values: [11, 22],
        current_alarms: [true, false, true, false],
        alarms_changed: true,
        tamper_just_detected: false,
        battery_level: 4012,
        tamper_detected: true,
        power_connected: false,
    };

    apply_monitor_update(&mut state, &update);

    assert_eq!(state.adc_values, [11, 22]);
    assert_eq!(state.current_alarms, [true, false, true, false]);
    assert_eq!(state.battery_level, 4012);
    assert!(state.tamper_detected);
    assert!(!state.power_connected);
    assert!(state.logic.pending_alive_message);
}
