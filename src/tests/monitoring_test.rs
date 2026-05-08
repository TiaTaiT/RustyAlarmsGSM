use crate::app_logic::LogicState;
use crate::monitoring::{MonitorUpdate, SensorSnapshot, apply_monitor_update, build_alarm_state, evaluate_monitor_update};
use crate::system_state::SystemState;

#[test]
fn build_alarm_state_uses_thresholds_and_closed_tamper_flag() {
    let alarms = build_alarm_state([1500, 500, 2999], false);
    assert_eq!(alarms, [true, false, true, true]);
}

#[test]
fn evaluate_monitor_update_detects_alarm_changes_and_tamper_edges() {
    let previous = SystemState {
        logic: LogicState::new(),
        battery_level: 3900,
        tamper_detected: false,
        adc_values: [0; 3],
        current_alarms: [false; 4],
        power_connected: true,
    };

    let update = evaluate_monitor_update(
        &previous,
        SensorSnapshot {
            battery_level: 4050,
            tamper_detected: true,
            power_connected: false,
            adc_values: [1500, 500, 3500],
        },
    );

    assert_eq!(update.current_alarms, [true, false, false, false]);
    assert!(update.alarms_changed);
    assert!(update.tamper_just_detected);
    assert_eq!(update.battery_level, 4050);
    assert!(!update.power_connected);
}

#[test]
fn apply_monitor_update_replaces_runtime_fields_without_touching_logic_state() {
    let mut state = SystemState::new();
    state.logic.pending_alive_message = true;

    let update = MonitorUpdate {
        adc_values: [11, 22, 33],
        current_alarms: [true, false, true, false],
        alarms_changed: true,
        tamper_just_detected: false,
        battery_level: 4012,
        tamper_detected: true,
        power_connected: false,
    };

    apply_monitor_update(&mut state, &update);

    assert_eq!(state.adc_values, [11, 22, 33]);
    assert_eq!(state.current_alarms, [true, false, true, false]);
    assert_eq!(state.battery_level, 4012);
    assert!(state.tamper_detected);
    assert!(!state.power_connected);
    assert!(state.logic.pending_alive_message);
}
