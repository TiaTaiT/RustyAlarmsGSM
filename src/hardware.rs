// /src/hardware.rs
// Hardware Abstraction Layer
// This module encapsulates all hardware-specific details of the board.
// Do not expose raw peripherals or pins outside this module. Instead, provide high-level methods on the public structs defined here.
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_stm32::adc::{Adc, AnyAdcChannel, SampleTime};
use embassy_stm32::adc::AdcChannel; // Required for .degrade_adc()
use embassy_stm32::gpio::{Input, Level, Output, Pull, Speed};
use embassy_stm32::mode::Async;
use embassy_stm32::peripherals::{ADC1, USB};
use embassy_stm32::rcc::{Hse, HseMode, Pll, PllDiv, PllMul, PllSource, Sysclk};
use embassy_stm32::rcc::mux;
use embassy_stm32::time::Hertz;
use embassy_stm32::usart::{Config as UartConfig, Uart, UartRx, UartTx};
use embassy_stm32::usb::Driver as UsbDriver;
use embassy_stm32::{Config, adc, bind_interrupts, dma, usart, usb};
use embassy_usb::Builder as UsbBuilder;
use embassy_usb::Config as UsbConfig;
use embassy_usb::UsbDevice;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State as CdcState};
use defmt::info;
use crate::constants::BATTERY_VOLTAGE_FACTOR;


// --- Internal Interrupt Binding ---
bind_interrupts!(struct Irqs {
    ADC1_COMP => adc::InterruptHandler<ADC1>;
    USART1    => usart::InterruptHandler<embassy_stm32::peripherals::USART1>;
    USART2    => usart::InterruptHandler<embassy_stm32::peripherals::USART2>;
    DMA1_CHANNEL2_3 => dma::InterruptHandler<embassy_stm32::peripherals::DMA1_CH2>, dma::InterruptHandler<embassy_stm32::peripherals::DMA1_CH3>;
	DMA1_CHANNEL4_5_6_7 => dma::InterruptHandler<embassy_stm32::peripherals::DMA1_CH4>, dma::InterruptHandler<embassy_stm32::peripherals::DMA1_CH5>;
    USB       => usb::InterruptHandler<USB>;
});

// --- Public Type Aliases ---
pub type ModemRx = UartRx<'static, Async>;
pub type ModemTx = UartTx<'static, Async>;

/// The concrete USB driver type for this board.
pub type BoardUsbDriver = UsbDriver<'static, USB>;

/// A ready-to-run USB CDC-ACM serial class.
pub type UsbSerial<'d> = CdcAcmClass<'d, BoardUsbDriver>;

// --- Enums ---
#[derive(Copy, Clone, PartialEq)]
pub enum PowerState {
    On,
    Off,
}

// Helper to avoid borrow-checker conflicts when mutating pins
fn apply_state(pin: &mut Output<'static>, state: PowerState) {
    match state {
        PowerState::On  => pin.set_high(),
        PowerState::Off => pin.set_low(),
    }
}

// --- Component: Status LEDs ---
pub struct StatusLeds {
    sys_led: Output<'static>,
    gsm_led: Output<'static>,
    err_led: Output<'static>,
    act_led: Output<'static>,
}

impl StatusLeds {
    pub fn set_system(&mut self, state: PowerState) { apply_state(&mut self.sys_led, state); }
    pub fn set_gsm   (&mut self, state: PowerState) { apply_state(&mut self.gsm_led, state); }
    pub fn set_error (&mut self, state: PowerState) { apply_state(&mut self.err_led, state); }
    pub fn set_action(&mut self, state: PowerState) { apply_state(&mut self.act_led, state); }

    pub fn set_by_index(&mut self, index: usize, state: PowerState) {
        match index {
            1 => self.set_system(state),
            2 => self.set_gsm(state),
            3 => self.set_error(state),
            4 => self.set_action(state),
            _ => {}
        }
    }
}

// --- Component: Modem Control ---
pub struct ModemControl {
    dc_power:  Output<'static>,
    power_key: Output<'static>,
    uart_dtr:  Output<'static>,
}

impl ModemControl {
    pub fn set_power_key(&mut self, state: PowerState) { apply_state(&mut self.power_key, state); }
    pub fn set_dc_power (&mut self, state: PowerState) { apply_state(&mut self.dc_power,  state); }
}

// --- Component: Alarm Relays (receiver only) ---
#[cfg(feature = "receiver")]
pub struct AlarmRelays {
    alarms: [Output<'static>; 4],
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

// --- Component: System Sensors ---
pub struct SystemSensors {
    #[cfg(feature = "transmitter")]
    alarms: [AnyAdcChannel<'static, ADC1>; 3],

    adc:            Adc<'static, ADC1>,
    battery_pin:    AnyAdcChannel<'static, ADC1>,
    power_good_pin: Input<'static>,
    tamper_pin:     Input<'static>,
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
    pub fn is_housing_open   (&self) -> bool { self.tamper_pin.is_high()     }
}

pub struct AlarmsControl {
    alarms_pullup: Output<'static>,
}

impl AlarmsControl {
    pub fn set_pullup(&mut self, state: PowerState) { apply_state(&mut self.alarms_pullup, state); }
}

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_usb::Handler;

pub static USB_CONNECTED: AtomicBool = AtomicBool::new(false);
pub static USB_STATE_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();

pub struct DeviceHandler;

impl DeviceHandler {
    pub const fn new() -> Self {
        Self
    }
}

impl Handler for DeviceHandler {
    fn reset(&mut self) {
        USB_CONNECTED.store(false, Ordering::Relaxed);
        USB_STATE_SIGNAL.signal(());
    }

    fn configured(&mut self, configured: bool) {
        USB_CONNECTED.store(configured, Ordering::Relaxed);
        USB_STATE_SIGNAL.signal(());
    }

    fn suspended(&mut self, suspended: bool) {
        if suspended {
            USB_CONNECTED.store(false, Ordering::Relaxed);
            USB_STATE_SIGNAL.signal(());
        }
    }
}

/// Static buffers consumed by `embassy-usb`. Must live for `'static`.
/// Allocate once via `StaticCell<UsbResources>`.
pub struct UsbResources {
    pub device_descriptor: [u8; 256],
    pub config_descriptor: [u8; 256],
    pub bos_descriptor:    [u8; 256],
    pub msos_descriptor:   [u8; 256],
    pub cdc_state:         CdcState<'static>,
    pub handler:           DeviceHandler,
}

impl UsbResources {
    pub const fn new() -> Self {
        Self {
            device_descriptor: [0u8; 256],
            config_descriptor: [0u8; 256],
            bos_descriptor:    [0u8; 256],
            msos_descriptor:   [0u8; 256],
            cdc_state:         CdcState::new(),
            handler:           DeviceHandler::new(),
        }
    }
}

/// Construct a `UsbDevice` and CDC-ACM serial class from the raw driver.
///
/// Call once from your dedicated USB task (see usage comment above).
/// Swap the VID/PID/strings for your own values before shipping.
pub fn build_usb(
    driver: BoardUsbDriver,
    res: &'static mut UsbResources,
) -> (UsbDevice<'static, BoardUsbDriver>, UsbSerial<'static>) {
    // -----------------------------------------------------------------------
    // IMPORTANT — USB clock on STM32L072:
    // The USB peripheral requires a 48 MHz source. On the L072 this comes from
    // the HSI48 oscillator (not the PLL). You must enable it in init():
    //
    //   use embassy_stm32::rcc::Hsi48Config;
    //   config.rcc.hsi48 = Some(Hsi48Config { sync_from_usb: true });
    //
    // And add the "hsi48" feature to embassy-stm32 in Cargo.toml:
    //   embassy-stm32 = { ..., features = [..., "hsi48"] }
    //
    // Without this the USB peripheral will not enumerate.
    // -----------------------------------------------------------------------

    let mut cfg = UsbConfig::new(
        0x16c0, // VID — replace (see https://pid.codes for open-source projects)
        0x27dd, // PID — replace
    );
    cfg.manufacturer  = Some("YourCompany");
    cfg.product       = Some("CDC Serial");
    cfg.serial_number = Some("00000001");
    cfg.max_power         = 100; // mA draw reported to host (≤500 for bus-powered)
    cfg.max_packet_size_0 = 64;

    let mut builder = UsbBuilder::new(
        driver,
        cfg,
        &mut res.device_descriptor,
        &mut res.config_descriptor,
        &mut res.bos_descriptor,
        &mut res.msos_descriptor,
    );

    builder.handler(&mut res.handler);

    let serial = CdcAcmClass::new(&mut builder, &mut res.cdc_state, 64);
    let device  = builder.build();

    (device, serial)
}

// --- Main Hardware Struct ---
pub struct Hardware {
    pub sensors:    SystemSensors,

    #[cfg(feature = "receiver")]
    pub relays:     AlarmRelays,

    #[cfg(feature = "transmitter")]
    pub alarms_ctrl: AlarmsControl,

    pub leds:       StatusLeds,
    pub modem_ctrl: ModemControl,
    pub modem_tx:   ModemTx,
    pub modem_rx:   ModemRx,

    /// Raw USB driver. Take it out with `.take()` and pass to a spawned
    /// `usb_task` — see the usage comment on `build_usb` above.
    pub usb_driver: Option<BoardUsbDriver>,

    // Debug UART — initialized but not actively used in the main task
    pub _debug_uart: Uart<'static, Async>,
}

// --- Initialization ---
pub fn init() -> Hardware {
    let mut config = Config::default();
    config.rcc.hse = Some(Hse { freq: Hertz::mhz(4), mode: HseMode::Oscillator });
    config.rcc.pll = Some(Pll { source: PllSource::HSE, div: PllDiv::DIV2, mul: PllMul::MUL4 });
    config.rcc.sys = Sysclk::PLL1_R;
    // The STM32L072 has no hsi48 field on rcc::Config — HSI48 is enabled
    // implicitly by the embassy-stm32 RCC driver when you select it as the
    // USB clock source via the mux. This is the correct approach for L0 family.
    config.rcc.mux.clk48sel = mux::Clk48sel::HSI48;

    let p = embassy_stm32::init(config);
    info!("Hardware initialized! Clocked at 4 MHz");

    // --- Outputs ---
    let alarms_ctrl = AlarmsControl {
        alarms_pullup: Output::new(p.PB1, Level::High, Speed::Low),
    };

    let modem_ctrl = ModemControl {
        dc_power:  Output::new(p.PB4, Level::High, Speed::Low),
        power_key: Output::new(p.PB6, Level::Low,  Speed::Low),
        uart_dtr:  Output::new(p.PB8, Level::Low,  Speed::Low),
    };

    let leds = StatusLeds {
        sys_led: Output::new(p.PB12, Level::Low, Speed::Low),
        gsm_led: Output::new(p.PB13, Level::Low, Speed::Low),
        err_led: Output::new(p.PB14, Level::Low, Speed::Low),
        act_led: Output::new(p.PB15, Level::Low, Speed::Low),
    };

    // --- UART ---
    let mut config_u1 = UartConfig::default();
    config_u1.baudrate = 115200;
    let _debug_uart = Uart::new(
        p.USART1, p.PA10, p.PA9, 
        p.DMA1_CH2, p.DMA1_CH3,
        Irqs,
        config_u1,
    ).unwrap();

    let mut config_u2 = UartConfig::default();
    config_u2.baudrate = 9600;
    let (modem_tx, modem_rx) = Uart::new(
        p.USART2, p.PA3, p.PA2, 
        p.DMA1_CH4, p.DMA1_CH5, 
        Irqs, 
        config_u2,
    ).unwrap().split();

    // --- ADC / Sensors ---
    let adc            = Adc::new(p.ADC1, Irqs);
    let battery_pin    = p.PB0.degrade_adc();
    let power_good_pin = Input::new(p.PB11, Pull::None);
    let tamper_pin     = Input::new(p.PB5,  Pull::None);

    #[cfg(feature = "transmitter")]
    let sensors = SystemSensors {
        alarms: [
            p.PA4.degrade_adc(),
            p.PA5.degrade_adc(),
            p.PA6.degrade_adc(),
        ],
        adc, battery_pin, power_good_pin, tamper_pin,
    };

    #[cfg(feature = "receiver")]
    let sensors = SystemSensors {
        adc, battery_pin, power_good_pin, tamper_pin,
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

    // --- USB Driver ---
    // STM32L072 USB pins: PA11 = D- (DM), PA12 = D+ (DP)
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