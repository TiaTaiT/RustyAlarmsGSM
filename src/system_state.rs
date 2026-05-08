use crate::app_logic::LogicState;

#[derive(Clone)]
pub struct SystemState {
    pub logic: LogicState,
    pub battery_level: u16,
    pub tamper_detected: bool,
    pub adc_values: [u16; 3],
    pub current_alarms: [bool; 4],
    pub power_connected: bool,
}

impl SystemState {
    pub const fn new() -> Self {
        Self {
            logic: LogicState::new(),
            battery_level: 0,
            tamper_detected: false,
            adc_values: [0; 3],
            current_alarms: [false; 4],
            power_connected: false,
        }
    }
}
