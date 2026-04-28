use crate::mcu_commands::{SystemSnapshot, format_mcu_reply};

#[test]
fn formats_common_mcu_replies() {
    let snapshot = SystemSnapshot {
        battery_level: 4123,
        tamper_detected: true,
        power_connected: false,
        #[cfg(feature = "transmitter")]
        adc_values: [1, 2, 3],
        #[cfg(feature = "transmitter")]
        current_alarms: [true, false, true, false],
        #[cfg(feature = "receiver")]
        relay_bits: 0b0101,
    };

    assert_eq!(format_mcu_reply(&snapshot, "_battery").as_str(), "\r\nBattery: 4123 mV\r\n");
    assert_eq!(format_mcu_reply(&snapshot, "_power").as_str(), "\r\nPower: Disconnected\r\n");
    assert_eq!(format_mcu_reply(&snapshot, "_tamper").as_str(), "\r\nTamper: Open\r\n");
}

#[test]
fn formats_unknown_mcu_command() {
    let snapshot = SystemSnapshot {
        battery_level: 0,
        tamper_detected: false,
        power_connected: true,
        #[cfg(feature = "transmitter")]
        adc_values: [0, 0, 0],
        #[cfg(feature = "transmitter")]
        current_alarms: [false; 4],
        #[cfg(feature = "receiver")]
        relay_bits: 0,
    };

    assert_eq!(
        format_mcu_reply(&snapshot, "_noop").as_str(),
        "\r\nUnknown MCU command: _noop\r\n"
    );
}
