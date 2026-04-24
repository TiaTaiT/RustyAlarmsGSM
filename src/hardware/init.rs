// /src/hardware/init.rs
use embassy_stm32::adc::{Adc, AdcChannel};
use embassy_stm32::gpio::{Input, Level, Output, Pull, Speed};
use embassy_stm32::mode::Async;
use embassy_stm32::peripherals::{ADC1, USB};
use embassy_stm32::rcc::mux;
use embassy_stm32::rcc::{Hse, HseMode, Pll, PllDiv, PllMul, PllSource, Sysclk};
use embassy_stm32::time::Hertz;
use embassy_stm32::usart::{Config as UartConfig, Uart};
use embassy_stm32::usb::Driver as UsbDriver;
use embassy_stm32::{adc, bind_interrupts, dma, usart, usb, Config};
use defmt::info;

use crate::constants::SYSCLK_MHZ;

use super::alarms::AlarmsControl;
use super::leds::StatusLeds;
use super::modem::{ModemControl, ModemRx, ModemTx};
#[cfg(feature = "receiver")]
use super::relays::AlarmRelays;
use super::sensors::SystemSensors;
use super::usb::BoardUsbDriver;

bind_interrupts!(struct Irqs {
    ADC1_COMP => adc::InterruptHandler<ADC1>;
    USART1    => usart::InterruptHandler<embassy_stm32::peripherals::USART1>;
    USART2    => usart::InterruptHandler<embassy_stm32::peripherals::USART2>;
    DMA1_CHANNEL2_3 => dma::InterruptHandler<embassy_stm32::peripherals::DMA1_CH2>, dma::InterruptHandler<embassy_stm32::peripherals::DMA1_CH3>;
    DMA1_CHANNEL4_5_6_7 => dma::InterruptHandler<embassy_stm32::peripherals::DMA1_CH4>, dma::InterruptHandler<embassy_stm32::peripherals::DMA1_CH5>;
    USB       => usb::InterruptHandler<USB>;
});

pub struct Hardware {
    pub sensors: SystemSensors,

    #[cfg(feature = "receiver")]
    pub relays: AlarmRelays,

    #[cfg(feature = "transmitter")]
    pub alarms_ctrl: AlarmsControl,

    pub leds: StatusLeds,
    pub modem_ctrl: ModemControl,
    pub modem_tx: ModemTx,
    pub modem_rx: ModemRx,
    pub usb_driver: Option<BoardUsbDriver>,
    pub _debug_uart: Uart<'static, Async>,
}

pub fn init() -> Hardware {
    let mut config = Config::default();
    config.rcc.hse = Some(Hse { freq: Hertz::mhz(SYSCLK_MHZ), mode: HseMode::Oscillator });
    config.rcc.pll = Some(Pll { source: PllSource::HSE, div: PllDiv::DIV2, mul: PllMul::MUL4 });
    config.rcc.sys = Sysclk::PLL1_R;
    config.rcc.mux.clk48sel = mux::Clk48sel::HSI48;

    let p = embassy_stm32::init(config);
    info!("Hardware initialized! Clocked at {} MHz", SYSCLK_MHZ);

    let alarms_ctrl = AlarmsControl {
        alarms_pullup: Output::new(p.PB1, Level::High, Speed::Low),
        is_sms_option: Input::new(p.PB10, Pull::None),
    };

    let modem_ctrl = ModemControl {
        dc_power: Output::new(p.PB4, Level::High, Speed::Low),
        power_key: Output::new(p.PB6, Level::Low, Speed::Low),
        uart_dtr: Output::new(p.PB8, Level::Low, Speed::Low),
    };

    let leds = StatusLeds {
        sys_led: Output::new(p.PB12, Level::Low, Speed::Low),
        alarm1_led: Output::new(p.PB13, Level::Low, Speed::Low),
        alarm2_led: Output::new(p.PB14, Level::Low, Speed::Low),
        alarm3_led: Output::new(p.PB15, Level::Low, Speed::Low),
    };

    let mut config_u1 = UartConfig::default();
    config_u1.baudrate = 115200;
    let _debug_uart = Uart::new(
        p.USART1,
        p.PA10,
        p.PA9,
        p.DMA1_CH2,
        p.DMA1_CH3,
        Irqs,
        config_u1,
    ).unwrap();

    let mut config_u2 = UartConfig::default();
    config_u2.baudrate = 9600;
    let (modem_tx, modem_rx) = Uart::new(
        p.USART2,
        p.PA3,
        p.PA2,
        p.DMA1_CH4,
        p.DMA1_CH5,
        Irqs,
        config_u2,
    ).unwrap().split();

    let adc = Adc::new(p.ADC1, Irqs);
    let battery_pin = p.PB0.degrade_adc();
    let power_good_pin = Input::new(p.PB11, Pull::None);
    let tamper_pin = Input::new(p.PB5, Pull::None);

    #[cfg(feature = "transmitter")]
    let sensors = SystemSensors {
        alarms: [p.PA4.degrade_adc(), p.PA5.degrade_adc(), p.PA6.degrade_adc()],
        adc,
        battery_pin,
        power_good_pin,
        tamper_pin,
    };

    #[cfg(feature = "receiver")]
    let sensors = SystemSensors {
        adc,
        battery_pin,
        power_good_pin,
        tamper_pin,
    };

    #[cfg(feature = "receiver")]
    let relays = AlarmRelays {
        alarms: [
            Output::new(p.PA4, Level::Low, Speed::Low),
            Output::new(p.PA5, Level::Low, Speed::Low),
            Output::new(p.PA6, Level::Low, Speed::Low),
            Output::new(p.PA7, Level::Low, Speed::Low),
        ],
    };

    let usb_driver = UsbDriver::new(p.USB, Irqs, p.PA12, p.PA11);

    Hardware {
        sensors,
        #[cfg(feature = "receiver")]
        relays,
        #[cfg(feature = "transmitter")]
        alarms_ctrl,
        leds,
        modem_ctrl,
        modem_tx,
        modem_rx,
        usb_driver: Some(usb_driver),
        _debug_uart,
    }
}