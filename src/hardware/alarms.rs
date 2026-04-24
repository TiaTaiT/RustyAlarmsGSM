// /src/hardware/alarms.rs
use embassy_stm32::gpio::{Input, Output};

use super::traits::{apply_state, PowerState};
#[cfg(feature = "transmitter")]
use super::traits::AlarmControlInterface;

pub struct AlarmsControl {
    pub(crate) alarms_pullup: Output<'static>,
    pub(crate) is_sms_option: Input<'static>,
}

impl AlarmsControl {
    pub fn set_pullup(&mut self, state: PowerState) { apply_state(&mut self.alarms_pullup, state); }
    pub fn is_sms_enabled(&self) -> bool { self.is_sms_option.is_high() }
}

#[cfg(feature = "transmitter")]
impl AlarmControlInterface for AlarmsControl {
    fn set_pullup(&mut self, state: PowerState) { AlarmsControl::set_pullup(self, state); }
    fn is_sms_enabled(&self) -> bool { AlarmsControl::is_sms_enabled(self) }
}