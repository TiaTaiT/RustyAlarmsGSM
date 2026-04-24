// /src/hardware/relays.rs
#[cfg(feature = "receiver")]
use embassy_stm32::gpio::Output;

#[cfg(feature = "receiver")]
use super::traits::{apply_state, PowerState, RelayInterface};

#[cfg(feature = "receiver")]
pub struct AlarmRelays {
    pub(crate) alarms: [Output<'static>; 4],
}

#[cfg(feature = "receiver")]
impl AlarmRelays {
    pub fn set(&mut self, index: usize, state: PowerState) {
        if index < 4 {
            apply_state(&mut self.alarms[index], state);
        }
    }

    pub fn set_all(&mut self, state: PowerState) {
        for pin in self.alarms.iter_mut() {
            apply_state(pin, state);
        }
    }
}

#[cfg(feature = "receiver")]
impl RelayInterface for AlarmRelays {
    fn set(&mut self, index: usize, state: PowerState) { AlarmRelays::set(self, index, state); }
    fn set_all(&mut self, state: PowerState) { AlarmRelays::set_all(self, state); }
}