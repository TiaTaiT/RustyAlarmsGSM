use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::pipe::Pipe;
use heapless::String;

use crate::mcu_commands::{SystemSnapshot, format_mcu_reply};

pub static USB_RX_PIPE: Pipe<CriticalSectionRawMutex, 256> = Pipe::new();
pub static USB_TX_PIPE: Pipe<CriticalSectionRawMutex, 1024> = Pipe::new();

pub struct RuntimeSnapshot {
    pub battery_level: u16,
    pub tamper_detected: bool,
    pub power_connected: bool,
    #[cfg(feature = "transmitter")]
    pub adc_values: [u16; 3],
    #[cfg(feature = "transmitter")]
    pub current_alarms: [bool; 4],
    #[cfg(feature = "receiver")]
    pub relay_bits: u8,
}

pub static RUNTIME_SNAPSHOT: Mutex<CriticalSectionRawMutex, RuntimeSnapshot> = Mutex::new(RuntimeSnapshot {
    battery_level: 0,
    tamper_detected: false,
    power_connected: false,
    #[cfg(feature = "transmitter")]
    adc_values: [0; 3],
    #[cfg(feature = "transmitter")]
    current_alarms: [false; 4],
    #[cfg(feature = "receiver")]
    relay_bits: 0,
});

#[cfg(test)]
pub async fn execute_mcu_command(_cmd: &str) {}

#[cfg(not(test))]
pub async fn execute_mcu_command(cmd: &str) {
    let snapshot = RUNTIME_SNAPSHOT.lock().await;
    let reply = format_mcu_reply(
        &SystemSnapshot {
            battery_level: snapshot.battery_level,
            tamper_detected: snapshot.tamper_detected,
            power_connected: snapshot.power_connected,
            #[cfg(feature = "transmitter")]
            adc_values: snapshot.adc_values,
            #[cfg(feature = "transmitter")]
            current_alarms: snapshot.current_alarms,
            #[cfg(feature = "receiver")]
            relay_bits: snapshot.relay_bits,
        },
        cmd,
    );

    write_usb_reply(&reply).await;
}

#[cfg(not(test))]
pub async fn write_usb_reply(reply: &String<128>) {
    let bytes = reply.as_bytes();
    let mut offset = 0;
    while offset < bytes.len() {
        let space = core::cmp::min(bytes.len() - offset, 64);
        let _ = USB_TX_PIPE.write(&bytes[offset..offset + space]).await;
        offset += space;
    }
}
