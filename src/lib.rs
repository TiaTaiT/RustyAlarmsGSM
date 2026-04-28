// File: src/lib.rs
#![cfg_attr(not(test), no_std)]

// 1. Expose the module we want to test
pub mod custom_strings;
pub mod constants;
pub mod alarms_handler;
pub mod date_converter;
pub mod gsm_time_converter;

// 2. Attach our test folder here instead!
#[cfg(test)]
mod tests;
