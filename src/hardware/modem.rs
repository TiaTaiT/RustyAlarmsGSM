// /src/hardware/mod.rs
use embassy_stm32::gpio::Output;
use embassy_stm32::mode::Async;
use embassy_stm32::usart::{UartRx, UartTx};

use super::traits::{apply_state, ModemControlInterface, PowerState};

pub type ModemRx = UartRx<'static, Async>;
pub type ModemTx = UartTx<'static, Async>;

pub struct ModemControl {
    pub(crate) dc_power: Output<'static>,
    pub(crate) power_key: Output<'static>,
    pub(crate) uart_dtr: Output<'static>,
}

impl ModemControl {
    pub fn set_power_key(&mut self, state: PowerState) { apply_state(&mut self.power_key, state); }
    pub fn set_dc_power(&mut self, state: PowerState) { apply_state(&mut self.dc_power, state); }
}

impl ModemControlInterface for ModemControl {
    fn set_power_key(&mut self, state: PowerState) { ModemControl::set_power_key(self, state); }
    fn set_dc_power(&mut self, state: PowerState) { ModemControl::set_dc_power(self, state); }
}