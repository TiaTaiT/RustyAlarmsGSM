use crate::{app_logic::LogicState, constants::{ALARMS_CHANNELS_AMOUNT, INTRUSION_CHANNELS_AMOUNT}};

#[derive(Clone)]
pub struct SystemState {
    pub logic: LogicState,
    pub battery_level: u16,
    pub tamper_detected: bool,
    pub adc_values: [u16; INTRUSION_CHANNELS_AMOUNT],
    pub current_alarms: [bool; ALARMS_CHANNELS_AMOUNT],
    pub power_connected: bool,
}

impl SystemState {
    pub const fn new() -> Self {
        Self {
            logic: LogicState::new(),
            battery_level: 0,
            tamper_detected: false,
            adc_values: [0; INTRUSION_CHANNELS_AMOUNT],
            current_alarms: [false; ALARMS_CHANNELS_AMOUNT],
            power_connected: false,
        }
    }
}
