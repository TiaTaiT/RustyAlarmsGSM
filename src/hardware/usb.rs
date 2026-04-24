// /src/hardware/usb.rs
use core::sync::atomic::{AtomicBool, Ordering};

use embassy_stm32::peripherals::USB;
use embassy_stm32::usb::Driver as UsbDriver;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State as CdcState};
use embassy_usb::Builder as UsbBuilder;
use embassy_usb::Config as UsbConfig;
use embassy_usb::Handler;
use embassy_usb::UsbDevice;

pub type BoardUsbDriver = UsbDriver<'static, USB>;
pub type UsbSerial<'d> = CdcAcmClass<'d, BoardUsbDriver>;

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

pub struct UsbResources {
    pub device_descriptor: [u8; 256],
    pub config_descriptor: [u8; 256],
    pub bos_descriptor: [u8; 256],
    pub msos_descriptor: [u8; 256],
    pub cdc_state: CdcState<'static>,
    pub handler: DeviceHandler,
}

impl UsbResources {
    pub const fn new() -> Self {
        Self {
            device_descriptor: [0u8; 256],
            config_descriptor: [0u8; 256],
            bos_descriptor: [0u8; 256],
            msos_descriptor: [0u8; 256],
            cdc_state: CdcState::new(),
            handler: DeviceHandler::new(),
        }
    }
}

pub fn build_usb(
    driver: BoardUsbDriver,
    res: &'static mut UsbResources,
) -> (UsbDevice<'static, BoardUsbDriver>, UsbSerial<'static>) {
    let mut cfg = UsbConfig::new(0x16c0, 0x27dd);
    cfg.manufacturer = Some("Investstroy");
    cfg.product = Some("USB-UART Bridge");
    cfg.serial_number = Some("00000001");
    cfg.max_power = 100;
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
    let device = builder.build();

    (device, serial)
}