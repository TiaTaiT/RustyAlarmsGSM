// File: src/lib.rs
#![cfg_attr(not(test), no_std)]

// 1. Expose the module we want to test
pub mod custom_strings;
pub mod constants;
pub mod alarms_handler;
pub mod date_converter;
pub mod gsm_time_converter;
pub mod phone_book;
pub mod alarms;
pub mod sim800_parser;
pub mod sim800_logic;
pub mod app_logic;
pub mod mcu_commands;
pub mod runtime;
pub mod visualization;
pub mod sim800;

#[cfg(not(test))]
pub mod hardware;

#[cfg(test)]
pub mod hardware {
    pub use crate::hardware_test_support::{ModemControl, ModemRx, ModemTx};
    pub use crate::hardware_test_support::{ModemControlInterface, ModemRxInterface, ModemTxInterface, PowerState};
}

#[cfg(test)]
pub mod hardware_test_support;

// 2. Attach our test folder here instead!
#[cfg(test)]
mod tests;
