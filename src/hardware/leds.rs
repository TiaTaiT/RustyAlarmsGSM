// /src/hardware/leds.rs
use embassy_stm32::gpio::Output;

use super::traits::{apply_state, LedInterface, PowerState};

pub struct StatusLeds {
    pub(crate) sys_led: Output<'static>,
    pub(crate) alarm1_led: Output<'static>,
    pub(crate) alarm2_led: Output<'static>,
    pub(crate) alarm3_led: Output<'static>,
}

impl StatusLeds {
    pub fn set_system(&mut self, state: PowerState) { apply_state(&mut self.sys_led, state); }
    pub fn set_alarm1(&mut self, state: PowerState) { apply_state(&mut self.alarm1_led, state); }
    pub fn set_alarm2(&mut self, state: PowerState) { apply_state(&mut self.alarm2_led, state); }
    pub fn set_alarm3(&mut self, state: PowerState) { apply_state(&mut self.alarm3_led, state); }
}

impl LedInterface for StatusLeds {
    fn set_system(&mut self, state: PowerState) { StatusLeds::set_system(self, state); }
    fn set_alarm1(&mut self, state: PowerState) { StatusLeds::set_alarm1(self, state); }
    fn set_alarm2(&mut self, state: PowerState) { StatusLeds::set_alarm2(self, state); }
    fn set_alarm3(&mut self, state: PowerState) { StatusLeds::set_alarm3(self, state); }
}