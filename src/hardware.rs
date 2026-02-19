// /src/hardware.rs
// Hardware Abstraction Layer
use embassy_stm32::adc::{Adc, SampleTime, AnyAdcChannel};
use embassy_stm32::adc::AdcChannel; // <--- Fixed missing import for degrade_adc
use embassy_stm32::gpio::{Input, Level, Output, Pull, Speed};
use embassy_stm32::mode::Async;
use embassy_stm32::peripherals::{ADC1};
use embassy_stm32::rcc::{Hse, HseMode, Pll, PllDiv, PllMul, PllSource, Sysclk};
use embassy_stm32::time::Hertz;
use embassy_stm32::usart::{Config as UartConfig, Uart, UartRx, UartTx};
use embassy_stm32::{adc, bind_interrupts, usart, Config};
use defmt::info;

// --- Internal Interrupt Binding ---
bind_interrupts!(struct Irqs {
    ADC1_COMP => adc::InterruptHandler<ADC1>;
    USART1 => usart::InterruptHandler<embassy_stm32::peripherals::USART1>;
    USART2 => usart::InterruptHandler<embassy_stm32::peripherals::USART2>;
});

// --- Public Type Aliases ---
pub type ModemRx = UartRx<'static, Async>;
pub type ModemTx = UartTx<'static, Async>;

// --- Enums ---
#[derive(Copy, Clone, PartialEq)]
pub enum PowerState {
    On,
    Off
}

// Helper function to avoid borrow checker conflicts
fn apply_state(pin: &mut Output<'static>, state: PowerState) {
    match state {
        PowerState::On => pin.set_high(),
        PowerState::Off => pin.set_low(),
    }
}

// --- Component: LEDs ---
pub struct StatusLeds {
    sys_led: Output<'static>,
    gsm_led: Output<'static>,
    err_led: Output<'static>,
    act_led: Output<'static>,
}

impl StatusLeds {
    pub fn set_system(&mut self, state: PowerState) { apply_state(&mut self.sys_led, state); }
    pub fn set_gsm(&mut self, state: PowerState)    { apply_state(&mut self.gsm_led, state); }
    pub fn set_error(&mut self, state: PowerState)  { apply_state(&mut self.err_led, state); }
    pub fn set_action(&mut self, state: PowerState) { apply_state(&mut self.act_led, state); }

    // Compatibility helper
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
    dc_power:   Output<'static>, 
    power_key:  Output<'static>, 
    uart_dtr:   Output<'static>, 
}

impl ModemControl {
    pub fn set_power_key(&mut self, state: PowerState) {
        apply_state(&mut self.power_key, state);
    }

    pub fn set_dc_power(&mut self, state: PowerState) {
        apply_state(&mut self.dc_power, state);
    }
}

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
    
    // Helper to clear all
    pub fn set_all(&mut self, state: PowerState) {
        for pin in self.alarms.iter_mut() {
            apply_state(pin, state);
        }
    }
}

// --- Component: Sensors ---
// 2. System Sensors - Handles Inputs (ADC, Battery, Tamper)
// In Transmitter mode, it ALSO handles the Alarm Inputs (ADC Channels)
pub struct SystemSensors {
    #[cfg(feature = "transmitter")]
    alarms: [AnyAdcChannel<'static, ADC1>; 4],
    
    adc: Adc<'static, ADC1>,
    battery_pin: AnyAdcChannel<'static, ADC1>,
    power_good_pin: Input<'static>,
    tamper_pin: Input<'static>,
}

impl SystemSensors {
    #[cfg(feature = "transmitter")]
    pub async fn read_alarms(&mut self) -> [u16; 4] {
        let v0 = self.adc.read(&mut self.alarms[0], SampleTime::CYCLES160_5).await;
        let v1 = self.adc.read(&mut self.alarms[1], SampleTime::CYCLES160_5).await;
        let v2 = self.adc.read(&mut self.alarms[2], SampleTime::CYCLES160_5).await;
        let v3 = self.adc.read(&mut self.alarms[3], SampleTime::CYCLES160_5).await;
        [v0, v1, v2, v3]
    }

    pub async fn read_battery_voltage(&mut self) -> u16 {
        self.adc.read(&mut self.battery_pin, SampleTime::CYCLES160_5).await
    }

    pub fn is_power_connected(&mut self) -> bool {
        self.power_good_pin.is_high()
    }

    pub fn is_housing_open(&self) -> bool {
        self.tamper_pin.is_high()
    }
}

// --- Main Hardware Struct ---
pub struct Hardware {
    pub sensors: SystemSensors,

    #[cfg(feature = "receiver")]
    pub relays: AlarmRelays,

    pub leds: StatusLeds,
    pub modem_ctrl: ModemControl,
    pub modem_tx: ModemTx,
    pub modem_rx: ModemRx,
    // Debug UART is optional/unused in main, but initialized here
    pub _debug_uart: Uart<'static, Async>, 
}

// --- Initialization ---
pub fn init() -> Hardware {
    let mut config = Config::default();
    config.rcc.hse = Some(Hse { freq: Hertz::mhz(4), mode: HseMode::Oscillator });
    config.rcc.pll = Some(Pll { source: PllSource::HSE, div: PllDiv::DIV2, mul: PllMul::MUL4 });
    config.rcc.sys = Sysclk::PLL1_R;

    let p = embassy_stm32::init(config);
    info!("Hardware initialized! Clocked at 8 MHz");

    // 1. Outputs
    let _alarms_pullup = Output::new(p.PB1, Level::High, Speed::Low); 
    
    let modem_ctrl = ModemControl {
        dc_power:   Output::new(p.PB4, Level::High, Speed::Low),
        power_key:  Output::new(p.PB6, Level::Low, Speed::Low),
        uart_dtr:   Output::new(p.PB8, Level::Low, Speed::Low), 
    };

    let leds = StatusLeds {
        sys_led: Output::new(p.PB12, Level::Low, Speed::Low),
        gsm_led: Output::new(p.PB13, Level::Low, Speed::Low),
        err_led: Output::new(p.PB14, Level::Low, Speed::Low),
        act_led: Output::new(p.PB15, Level::Low, Speed::Low),
    };

    // 2. Comms
    let mut config_u1 = UartConfig::default();
    config_u1.baudrate = 115200;
    let _debug_uart = Uart::new(
        p.USART1,
        p.PA10,
        p.PA9,
        Irqs,
        p.DMA1_CH2,
        p.DMA1_CH3,
        config_u1
    ).unwrap();

    let mut config_u2 = UartConfig::default();
    config_u2.baudrate = 9600;
    let (modem_tx, modem_rx) = Uart::new(
        p.USART2,
        p.PA3,
        p.PA2,
        Irqs,
        p.DMA1_CH4,
        p.DMA1_CH5,
        config_u2
    ).unwrap().split();

    // 3. Sensors
    let adc = Adc::new(p.ADC1, Irqs);
    let battery_pin = p.PB0.degrade_adc();
    let power_good_pin = Input::new(p.PB11, Pull::None);
    let tamper_pin = Input::new(p.PB5, Pull::None);

   // FEATURE-SPECIFIC ASSIGNMENT
    #[cfg(feature = "transmitter")]
    let sensors = SystemSensors {
        alarms: [
            p.PA4.degrade_adc(),
            p.PA5.degrade_adc(),
            p.PA6.degrade_adc(),
            p.PA7.degrade_adc(),
        ],
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

    Hardware {
        sensors,
        #[cfg(feature = "receiver")]
        relays,
        leds,
        modem_ctrl,
        modem_tx,
        modem_rx,
        _debug_uart,
    }
}