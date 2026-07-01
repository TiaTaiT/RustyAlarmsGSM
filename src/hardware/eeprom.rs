// src/hardware/eeprom.rs
use embassy_stm32::pac;
use core::ptr;

pub struct Eeprom;

impl Eeprom {
    const EEPROM_BASE: u32 = 0x0808_0000;
    const EEPROM_SIZE: u32 = 6144; // 6 KB

    // Dedicated word-aligned offsets for your variables
    const OFFSET_ALIVE_PERIOD: u32 = 0x0000;
    const OFFSET_ALIVE_PERIOD_DELAY: u32 = 0x0004;

    /// Stores the alive_period in EEPROM
    pub fn write_alive_period(value: u32) {
        Self::write_u32(Self::OFFSET_ALIVE_PERIOD, value);
    }

    /// Retrieves the alive_period from EEPROM
    pub fn read_alive_period() -> u32 {
        Self::read_u32(Self::OFFSET_ALIVE_PERIOD)
    }

    /// Stores the alive_period_delay in EEPROM
    pub fn write_alive_period_delay(value: u32) {
        Self::write_u32(Self::OFFSET_ALIVE_PERIOD_DELAY, value);
    }

    /// Retrieves the alive_period_delay from EEPROM
    pub fn read_alive_period_delay() -> u32 {
        Self::read_u32(Self::OFFSET_ALIVE_PERIOD_DELAY)
    }

    /// Write a 32-bit word to the EEPROM
    pub fn write_u32(offset: u32, value: u32) {
        assert!(offset + 4 <= Self::EEPROM_SIZE, "EEPROM overflow");
        assert!(offset % 4 == 0, "Writes to u32 must be word-aligned");

        let flash = pac::FLASH;

        // 1. Unlock the Data EEPROM and the PECR (Power/EEPROM Control Register)
        if flash.pecr().read().pelock() {
            // Write standard STM32L0 EEPROM unlock keys
            flash.pekeyr().write_value(0x89AB_CDEF);
            flash.pekeyr().write_value(0x0203_0405);
        }

        // 2. Write the data directly to the memory-mapped address
        let addr = (Self::EEPROM_BASE + offset) as *mut u32;
        unsafe {
            ptr::write_volatile(addr, value);
        }

        // 3. Wait for the hardware to finish the physical write (BSY flag)
        while flash.sr().read().bsy() {
            // spin-wait (takes a few milliseconds)
        }

        // 4. Re-lock the EEPROM to prevent accidental writes
        flash.pecr().modify(|w| w.set_pelock(true));
    }

    /// Read a 32-bit word from the EEPROM
    pub fn read_u32(offset: u32) -> u32 {
        assert!(offset + 4 <= Self::EEPROM_SIZE);
        assert!(offset % 4 == 0);

        let addr = (Self::EEPROM_BASE + offset) as *const u32;
        
        // Reading EEPROM does not require unlocking
        unsafe { ptr::read_volatile(addr) }
    }
}