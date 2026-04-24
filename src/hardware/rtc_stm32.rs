// /src/hardware/rtc_stm32.rs
use embassy_stm32::pac::{PWR, RCC, RTC};

use super::traits::{GsmTime, Rtc};

/// STM32 RTC implementation using LSI
pub struct Stm32Rtc {
    _private: (),
}

impl Rtc for Stm32Rtc {
    fn init() -> Self {
        // Enable PWR clock
        RCC.apb1enr().modify(|w| w.set_pwren(true));

        // Enable backup domain access
        PWR.cr().modify(|w| w.set_dbp(true));

        // Enable LSI
        RCC.csr().modify(|w| w.set_lsion(true));
        while !RCC.csr().read().lsirdy() {}

        // Select LSI as RTC clock
        RCC.csr().modify(|w| {
            w.set_rtcsel(
                embassy_stm32::pac::rcc::vals::Rtcsel::LSI
            );
            w.set_rtcen(true);
        });

        let rtc = RTC;

        // Disable write protection
        rtc.wpr().write(|w| w.set_key(0xCA));
        rtc.wpr().write(|w| w.set_key(0x53));

        // Enter init mode
        rtc.isr().modify(|w| w.set_init(true));
        while !rtc.isr().read().initf() {}

        // Prescalers for ~1 Hz
        rtc.prer().modify(|w| {
            w.set_prediv_a(0x7F);
            w.set_prediv_s(0x0120);
        });

        // Exit init
        rtc.isr().modify(|w| w.set_init(false));

        rtc.wpr().write(|w| w.set_key(0xFF));

        Self { _private: () }
    }

    fn set_time(&mut self, time: GsmTime) {
        let rtc = RTC;

        rtc.wpr().write(|w| w.set_key(0xCA));
        rtc.wpr().write(|w| w.set_key(0x53));

        rtc.isr().modify(|w| w.set_init(true));
        while !rtc.isr().read().initf() {}

        rtc.dr().write(|w| {
            w.set_dt(time.day / 10);
            w.set_du(time.day % 10);
            w.set_mt((time.month / 10) > 0);
            w.set_mu(time.month % 10);
            w.set_yt(time.year / 10);
            w.set_yu(time.year % 10);
        });

        rtc.tr().write(|w| {
            w.set_ht(time.hour / 10);
            w.set_hu(time.hour % 10);
            w.set_mnt(time.minute / 10);
            w.set_mnu(time.minute % 10);
            w.set_st(time.second / 10);
            w.set_su(time.second % 10);
        });

        rtc.isr().modify(|w| w.set_init(false));

        rtc.wpr().write(|w| w.set_key(0xFF));
    }

    fn get_time(&self) -> GsmTime {
        let rtc = RTC;

        rtc.isr().modify(|w| w.set_rsf(false));
        while !rtc.isr().read().rsf() {}

        let tr = rtc.tr().read();
        let dr = rtc.dr().read();

        let day = dr.dt() * 10 + dr.du();
        let month = (dr.mt() as u8) * 10 + dr.mu();
        let year = dr.yt() * 10 + dr.yu();

        let hour = tr.ht() * 10 + tr.hu();
        let minute = tr.mnt() * 10 + tr.mnu();
        let second = tr.st() * 10 + tr.su();

        GsmTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
        }
    }
}