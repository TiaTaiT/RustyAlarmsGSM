// /src/hardware/mod.rs
pub mod alarms;
pub mod init;
pub mod leds;
pub mod modem;
pub mod rtc_stm32;
pub mod relays;
pub mod sensors;
pub mod traits;
pub mod usb;

pub use alarms::AlarmsControl;
pub use init::{init, Hardware};
pub use leds::StatusLeds;
pub use modem::{ModemControl, ModemRx, ModemTx};
pub use rtc_stm32::Stm32Rtc as RtcControl;
#[cfg(feature = "receiver")]
pub use relays::AlarmRelays;
pub use sensors::SystemSensors;
pub use traits::{LedInterface, ModemControlInterface, PowerState, Rtc, SensorInterface};
#[cfg(feature = "receiver")]
pub use traits::RelayInterface;
#[cfg(feature = "transmitter")]
pub use traits::AlarmControlInterface;
pub use usb::{build_usb, BoardUsbDriver, UsbResources, UsbSerial, USB_CONNECTED, USB_STATE_SIGNAL};