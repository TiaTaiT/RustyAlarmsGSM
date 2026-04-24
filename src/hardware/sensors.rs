// /src/hardware/sensors.rs
use embassy_stm32::adc::{Adc, AnyAdcChannel, SampleTime};
use embassy_stm32::gpio::Input;
use embassy_stm32::peripherals::ADC1;

use crate::constants::BATTERY_VOLTAGE_FACTOR;

use super::traits::SensorInterface;

pub struct SystemSensors {
    #[cfg(feature = "transmitter")]
    pub(crate) alarms: [AnyAdcChannel<'static, ADC1>; 3],
    pub(crate) adc: Adc<'static, ADC1>,
    pub(crate) battery_pin: AnyAdcChannel<'static, ADC1>,
    pub(crate) power_good_pin: Input<'static>,
    pub(crate) tamper_pin: Input<'static>,
}

impl SystemSensors {
    #[cfg(feature = "transmitter")]
    pub async fn read_alarms(&mut self) -> [u16; 3] {
        let v0 = self.adc.read(&mut self.alarms[0], SampleTime::CYCLES160_5).await;
        let v1 = self.adc.read(&mut self.alarms[1], SampleTime::CYCLES160_5).await;
        let v2 = self.adc.read(&mut self.alarms[2], SampleTime::CYCLES160_5).await;
        [v0, v1, v2]
    }

    pub async fn read_battery_voltage(&mut self) -> u16 {
        (self.adc.read(&mut self.battery_pin, SampleTime::CYCLES160_5).await as f32 * BATTERY_VOLTAGE_FACTOR) as u16
    }

    pub fn is_power_connected(&self) -> bool { self.power_good_pin.is_high() }
    pub fn is_housing_open(&self) -> bool { self.tamper_pin.is_high() }
}

impl SensorInterface for SystemSensors {
    #[cfg(feature = "transmitter")]
    async fn read_alarms(&mut self) -> [u16; 3] { SystemSensors::read_alarms(self).await }

    async fn read_battery_voltage(&mut self) -> u16 { SystemSensors::read_battery_voltage(self).await }
    fn is_power_connected(&self) -> bool { SystemSensors::is_power_connected(self) }
    fn is_housing_open(&self) -> bool { SystemSensors::is_housing_open(self) }
}