use crate::constants::{
    ALARMS_CHANNELS_AMOUNT,
    BATTERY_UNDERVOLTAGE_THRESHOLD,
    HIGH_INTRUSION_THRESHOLD,
    INTRUSION_CHANNELS_AMOUNT,
    LOW_INTRUSION_THRESHOLD
};
use crate::system_state::SystemState;

pub struct SensorSnapshot {
    pub battery_level: u16,
    pub tamper_detected: bool,
    pub power_connected: bool,
    pub adc_values: [u16; INTRUSION_CHANNELS_AMOUNT],
}

pub struct MonitorUpdate {
    pub adc_values: [u16; INTRUSION_CHANNELS_AMOUNT],
    pub current_alarms: [bool; ALARMS_CHANNELS_AMOUNT],
    pub alarms_changed: bool,
    pub tamper_just_detected: bool,
    pub battery_level: u16,
    pub tamper_detected: bool,
    pub power_connected: bool,
}

pub fn evaluate_monitor_update(
    previous: &SystemState,
    snapshot: SensorSnapshot,
) -> MonitorUpdate {
    let current_alarms = build_alarm_state(
        snapshot.adc_values, 
        snapshot.battery_level, 
        snapshot.tamper_detected);

    MonitorUpdate {
        adc_values: snapshot.adc_values,
        current_alarms,
        alarms_changed: previous.current_alarms != current_alarms,
        tamper_just_detected: snapshot.tamper_detected && !previous.tamper_detected,
        battery_level: snapshot.battery_level,
        tamper_detected: snapshot.tamper_detected,
        power_connected: snapshot.power_connected,
    }
}

pub fn apply_monitor_update(state: &mut SystemState, update: &MonitorUpdate) {
    state.adc_values = update.adc_values;
    state.current_alarms = update.current_alarms;
    state.battery_level = update.battery_level;
    state.tamper_detected = update.tamper_detected;
    state.power_connected = update.power_connected;
}

fn build_alarm_state(
    adc_values: [u16; INTRUSION_CHANNELS_AMOUNT],
    battery_level: u16,
    tamper_detected: bool) -> [bool; ALARMS_CHANNELS_AMOUNT] {
    [
        is_intrusion_active(adc_values[0]),
        is_intrusion_active(adc_values[1]),
        is_battery_ok(battery_level),
        !tamper_detected
    ]
}

fn is_intrusion_active(value: u16) -> bool {
    value > LOW_INTRUSION_THRESHOLD && value < HIGH_INTRUSION_THRESHOLD
}

fn is_battery_ok(value: u16) -> bool {
    value >= BATTERY_UNDERVOLTAGE_THRESHOLD
}
