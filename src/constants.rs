// /src/constants.rs

// ADC is 12-bit (0-4095). Adjust thresholds if voltage dividers changed.
pub const LOW_INTRUSION_THRESHOLD: u16 = 1000;
pub const HIGH_INTRUSION_THRESHOLD: u16 = 3000;

// Updated to 4 channels for the new board
pub const ALARMS_CHANNELS_AMOUNT: usize = 4;
pub const ALARMS_STACK_DEPTH: usize = 3;
pub const ALARMS_BUFFER_SIZE: usize = 256;
pub const ALARMS_MESSAGE_STRING_LENGTH: usize = 4; // Matches channels

pub const INIT_SIM800_DELAY_SECONDS: u32 = 6;
pub const ALIVE_PERIOD_MINUTES: i32 = 120;
pub const SYSTEM_MONITOR_PERIOD_HOURS: u32 = 12;

pub const SMS_PREFIX: &str = "PPP";
pub const SMS_DIVIDER: &str = "_";
pub const ONLINE_SIGNAL: &str = "*";
pub const CONFIRMATION_SIGNAL: &str = "#";
pub const ERROR_SIGNAL: &str = "0";
pub const DTMF_PACKET_LENGTH: usize = 4; // Matches channels

pub const MAX_PHONE_LENGTH: usize = 16;

pub const SIM800_LINE_BUFFER_SIZE: usize = 64;
pub const MAXIMUM_DTMF_BUFFER_SIZE: usize = 16;
pub const MAXIMUM_SIM800_LINE_COUNT: usize = 8;
pub const MAXIMUM_INCOMING_SMS_BUFFER_SIZE: usize = 8;

pub const SIM800_RX_BUFFER_SIZE: usize = 256;
pub const BATTERY_VOLTAGE_FACTOR: f32 = 9.155;

pub const CALLBACK_PERIOD_MINUTES: u32 = 5;

// STM32L0 config in hardware.rs is 4MHz
pub const SYSCLK_MHZ: u32 = 4;