// File: src/mcu_commands.rs
// Use this file for all USB terminal command handlers
use heapless::String;

#[cfg(not(test))]
use crate::hardware::Eeprom;

#[derive(Clone, PartialEq, Debug)]
pub struct SystemSnapshot {
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

pub fn format_mcu_reply(snapshot: &SystemSnapshot, cmd: &str) -> String<128> {
    let mut reply = String::<128>::new();
    use core::fmt::Write;

    let cmd_trimmed = cmd.trim_end();

    match cmd_trimmed {
        #[cfg(feature = "transmitter")]
        "_alarms" => {
            let a = snapshot.current_alarms;
            let _ = write!(
                reply,
                "\r\nAlarms: {}{}{}{}\r\n",
                a[0] as u8, a[1] as u8, a[2] as u8, a[3] as u8
            );
        }
        #[cfg(feature = "transmitter")]
        "_adc" => {
            let v = snapshot.adc_values;
            let _ = write!(reply, "\r\nADC: {}, {}, {}\r\n", v[0], v[1], v[2]);
        }
        #[cfg(feature = "receiver")]
        "_relays" => {
            let bits = snapshot.relay_bits;
            let _ = write!(
                reply,
                "\r\nRelays: {}{}{}{}\r\n",
                bits & 1,
                (bits >> 1) & 1,
                (bits >> 2) & 1,
                (bits >> 3) & 1
            );
        }
        "_battery" => {
            let _ = write!(reply, "\r\nBattery: {} mV\r\n", snapshot.battery_level);
        }
        "_power" => {
            let p = if snapshot.power_connected {
                "Connected"
            } else {
                "Disconnected"
            };
            let _ = write!(reply, "\r\nPower: {}\r\n", p);
        }
        "_tamper" => {
            let t = if snapshot.tamper_detected { "Open" } else { "Closed" };
            let _ = write!(reply, "\r\nTamper: {}\r\n", t);
        }
        "_alive" => {
            #[cfg(not(test))]
            let val = Eeprom::read_alive_period();
            #[cfg(test)]
            let val = 90;
            let _ = write!(reply, "\r\n{}\r\n", val);
        }
        c if c.starts_with("_alive=") => {
            if let Some(val_str) = c.strip_prefix("_alive=") {
                if let Ok(val) = val_str.parse::<u32>() {
                    #[cfg(not(test))]
                    Eeprom::write_alive_period(val);
                    let _ = write!(reply, "\r\nOK\r\n");
                } else {
                    let _ = write!(reply, "\r\nERROR\r\n");
                }
            } else {
                let _ = write!(reply, "\r\nERROR\r\n");
            }
        }
        #[cfg(feature = "receiver")]
        "_alivedelay" => {
            #[cfg(not(test))]
            let val = Eeprom::read_alive_period_delay();
            #[cfg(test)]
            let val = 20;
            let _ = write!(reply, "\r\n{}\r\n", val);
        }
        #[cfg(feature = "receiver")]
        c if c.starts_with("_alivedelay=") => {
            if let Some(val_str) = c.strip_prefix("_alivedelay=") {
                if let Ok(val) = val_str.parse::<u32>() {
                    #[cfg(not(test))]
                    Eeprom::write_alive_period_delay(val);
                    let _ = write!(reply, "\r\nOK\r\n");
                } else {
                    let _ = write!(reply, "\r\nERROR\r\n");
                }
            } else {
                let _ = write!(reply, "\r\nERROR\r\n");
            }
        }
        "_reboot" => {
            let _ = write!(reply, "\r\nOK\r\n");
        }
        _ => {
            let _ = write!(reply, "\r\nUnknown MCU command: {}\r\n", cmd_trimmed);
        }
    }

    reply
}