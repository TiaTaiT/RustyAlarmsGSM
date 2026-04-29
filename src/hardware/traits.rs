// /src/hardware/traits.rs
#![allow(async_fn_in_trait)]
use embassy_stm32::gpio::Output;

use crate::gsm_time_converter::GsmTime;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum PowerState {
    On,
    Off,
}

pub trait LedInterface {
    fn set_system(&mut self, state: PowerState);
    fn set_alarm1(&mut self, state: PowerState);
    fn set_alarm2(&mut self, state: PowerState);
    fn set_alarm3(&mut self, state: PowerState);

    fn set_by_index(&mut self, index: usize, state: PowerState) {
        match index {
            1 => self.set_system(state),
            2 => self.set_alarm1(state),
            3 => self.set_alarm2(state),
            4 => self.set_alarm3(state),
            _ => {}
        }
    }
}

pub trait ModemControlInterface {
    fn set_power_key(&mut self, state: PowerState);
    fn set_dc_power(&mut self, state: PowerState);
}

#[cfg(feature = "receiver")]
pub trait RelayInterface {
    fn set(&mut self, index: usize, state: PowerState);
    fn set_all(&mut self, state: PowerState);
}

pub trait SensorInterface {
    #[cfg(feature = "transmitter")]
    async fn read_alarms(&mut self) -> [u16; 3];

    async fn read_battery_voltage(&mut self) -> u16;
    fn is_power_connected(&self) -> bool;
    fn is_housing_open(&self) -> bool;
}

#[cfg(feature = "transmitter")]
pub trait AlarmControlInterface {
    fn set_pullup(&mut self, state: PowerState);
    fn is_sms_enabled(&self) -> bool;
}

pub(crate) fn apply_state(pin: &mut Output<'static>, state: PowerState) {
    match state {
        PowerState::On => pin.set_high(),
        PowerState::Off => pin.set_low(),
    }
}

/// Hardware-independent RTC interface
pub trait Rtc {
    /// Initialize RTC hardware
    fn init() -> Self
    where
        Self: Sized;

    /// Set RTC time
    fn set_time(&mut self, time: GsmTime);

    /// Get current RTC time
    fn get_time(&self) -> GsmTime;
}
