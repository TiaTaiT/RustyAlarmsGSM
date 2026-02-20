#![no_std]
#![no_main]

use defmt::{info, warn};
use defmt_rtt as _;
// use embassy_stm32::adc::SampleTime; // Removed, handled in hardware.rs
use panic_probe as _;

use embassy_executor::Spawner;
use embassy_futures::select::{select3, Either3};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Instant, Timer};
use heapless::String;

mod constants;
mod hardware;
mod alarms_handler;
mod rtc;
mod sim800;
mod gsm_time_converter;
mod date_converter;
mod phone_book;
mod custom_strings;

use crate::constants::*;
use crate::hardware::{Hardware, PowerState, StatusLeds, SystemSensors}; 
use crate::alarms_handler::{AlarmStack, AlarmTracker};
use crate::rtc::RtcControl;
use crate::sim800::{Command, Sim800Driver, SimEvent};
#[cfg(feature = "receiver")]
use crate::hardware::AlarmRelays;
use static_cell::StaticCell;

// --- Global Signals/Channels ---
static CMD_CHANNEL: Channel<CriticalSectionRawMutex, Command, 4> = Channel::new();
static EVENT_CHANNEL: Channel<CriticalSectionRawMutex, SimEvent, 4> = Channel::new();
static USB_STATE: StaticCell<hardware::UsbResources> = StaticCell::new();

struct SystemState {
    alarm_stack: AlarmStack,
    alive_countdown: i32,
    // Add battery state tracking if needed
    battery_level: u16,
    tamper_detected: bool,
}

static STATE: Mutex<CriticalSectionRawMutex, SystemState> = Mutex::new(SystemState {
    alarm_stack: AlarmStack::new(), 
    alive_countdown: 0,
    battery_level: 0,
    tamper_detected: false,
});

static RTC: Mutex<CriticalSectionRawMutex, Option<RtcControl>> = Mutex::new(None);

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // 1. Initialize new Hardware struct
    let mut hw: Hardware = hardware::init();
    
    let mut leds = hw.leds;
    let sim_control = hw.modem_ctrl;
    let sensors = hw.sensors;
    let tx = hw.modem_tx;
    let rx = hw.modem_rx;
    
    #[cfg(feature = "receiver")]
    let relays = hw.relays;

    // RTC Init
    {
        let rtc_ctrl = RtcControl::init();
        let mut rtc_lock = RTC.lock().await;
        *rtc_lock = Some(rtc_ctrl);
    }

    info!("Starting Embassy800c on STM32L0...");
    leds.set_system(PowerState::On);
    Timer::after(Duration::from_millis(200)).await;
    leds.set_system(PowerState::Off);

    spawner.spawn(sim800_task(tx, rx, sim_control)).unwrap();
    spawner.spawn(monitor_task(sensors)).unwrap();
    
    // Pass relays to logic task if receiver
    #[cfg(feature = "receiver")]
    spawner.spawn(logic_task(leds, relays)).unwrap();
    
    #[cfg(feature = "transmitter")]
    spawner.spawn(logic_task(leds)).unwrap();
    
    spawner.spawn(system_monitor_task()).unwrap();

    let driver = hw.usb_driver.take().unwrap();
    spawner.spawn(usb_task(driver)).unwrap();
}

#[embassy_executor::task]
   async fn usb_task(driver: hardware::BoardUsbDriver) {
       let res = USB_STATE.init(hardware::UsbResources::new());
       let (mut device, mut serial) = hardware::build_usb(driver, res);
       embassy_futures::join::join(device.run(), async {
           loop {
               serial.wait_connection().await;
               info!("USB connected");
               let mut buf = [0u8; 64];
               loop {
                   match serial.read_packet(&mut buf).await {
                       Ok(n)  => { serial.write_packet(&buf[..n]).await.ok(); }
                       Err(_) => break,
                   }
               }
           }
       }).await;
   }

#[embassy_executor::task]
async fn sim800_task(tx: hardware::ModemTx, rx: hardware::ModemRx, control: hardware::ModemControl) {
    let mut driver = Sim800Driver::new(tx, rx, control);
    
    // REMOVED: CMD_CHANNEL.send(Command::Init).await; 
    
    // We only need to request the time update, power_on happens automatically in driver.run()
    CMD_CHANNEL.send(Command::UpdateTime).await; 
    
    driver.run(CMD_CHANNEL.receiver(), EVENT_CHANNEL.sender()).await;
}

#[embassy_executor::task]
async fn monitor_task(mut sensors: SystemSensors) {
    loop {
        // Transmitter: Read Alarms
        #[cfg(feature = "transmitter")]
        {
            let values = sensors.read_alarms().await;
            let bools = [
                values[0] > LOW_INTRUSION_THRESHOLD && values[0] < HIGH_INTRUSION_THRESHOLD,
                values[1] > LOW_INTRUSION_THRESHOLD && values[1] < HIGH_INTRUSION_THRESHOLD,
                values[2] > LOW_INTRUSION_THRESHOLD && values[2] < HIGH_INTRUSION_THRESHOLD,
                values[3] > LOW_INTRUSION_THRESHOLD && values[3] < HIGH_INTRUSION_THRESHOLD,
            ];
            
            let mut state = STATE.lock().await;
            state.alarm_stack.push(&bools);
        }

        // Both: Read System health
        let battery = sensors.read_battery_voltage().await;
        let tamper = sensors.is_housing_open();
        
        {
            let mut state = STATE.lock().await;
            state.battery_level = battery;
            if tamper && !state.tamper_detected {
                warn!("TAMPER DETECTED!");
            }
            state.tamper_detected = tamper;
        }

        Timer::after(Duration::from_millis(500)).await;
    }
}

#[cfg(feature = "transmitter")]
#[embassy_executor::task]
async fn logic_task(mut leds: StatusLeds) {
    run_logic(leds).await;
}

#[cfg(feature = "receiver")]
#[embassy_executor::task]
async fn logic_task(leds: StatusLeds, mut relays: AlarmRelays) {
    run_logic(leds, &mut relays).await;
}

async fn run_logic(
    mut leds: StatusLeds, 
    #[cfg(feature = "receiver")] relays: &mut AlarmRelays
) {
    let mut watchdog_deadline: Option<Instant> = None;
    let mut dtmf_buffer = String::<DTMF_PACKET_LENGTH>::new();
    let mut next_sender_tick = Instant::now() + Duration::from_secs(60);

    loop {
        let watchdog_fut = async {
             if let Some(deadline) = watchdog_deadline { Timer::at(deadline).await; true } 
             else { core::future::pending::<bool>().await }
        };
        let sender_fut = Timer::at(next_sender_tick);
        let event_fut = EVENT_CHANNEL.receive();

        match select3(event_fut, sender_fut, watchdog_fut).await {
            // --- CASE 1: EVENTS (Both) ---
            Either3::First(event) => {
                match event {
                    SimEvent::SmsReceived { message, .. } => {
                        if let Some(alarm_str) = custom_strings::extract_before_delimiter(&message, ";") {
                             if alarm_str.len() == ALARMS_MESSAGE_STRING_LENGTH {
                                 #[cfg(feature = "transmitter")]
                                 visualize_leds(&mut leds, alarm_str).await;

                                 #[cfg(feature = "receiver")]
                                 visualize_relays(relays, &mut leds, alarm_str).await;

                                 watchdog_deadline = Some(Instant::now() + Duration::from_secs(255 * 60));
                             }
                        }
                    },
                    SimEvent::DtmfReceived(c) => {
                        if dtmf_buffer.push(c).is_ok() {
                            if dtmf_buffer.len() == DTMF_PACKET_LENGTH {
                                #[cfg(feature = "transmitter")]
                                visualize_leds(&mut leds, &dtmf_buffer).await;

                                #[cfg(feature = "receiver")]
                                visualize_relays(relays, &mut leds, &dtmf_buffer).await;

                                watchdog_deadline = Some(Instant::now() + Duration::from_secs(255 * 60));
                                dtmf_buffer.clear();
                            }
                        }
                    },
                    SimEvent::CallEnded => { dtmf_buffer.clear(); },
                    SimEvent::CallReceived { number } => {
                        CMD_CHANNEL.send(Command::HandleIncomingCall { phone_number: number }).await;
                    },
                    SimEvent::CallExecuted(success) => {
                        if success { 
                            leds.set_action(PowerState::On);
                            Timer::after(Duration::from_secs(1)).await;
                            leds.set_action(PowerState::Off);
                        }
                    },
                    SimEvent::TimeReceived(time) => {
                         let mut rtc = RTC.lock().await;
                         if let Some(ref mut rtc_ctrl) = *rtc { rtc_ctrl.set_time(time); }
                    }
                }
            },

            // --- CASE 2: SENDER LOGIC TICK ---
            Either3::Second(_) => {
                next_sender_tick += Duration::from_secs(60);
                
                #[cfg(feature = "transmitter")]
                {
                    let mut pending_dtmf: Option<String<DTMF_PACKET_LENGTH>> = None;
                    let mut pending_sms: Option<String<SIM800_LINE_BUFFER_SIZE>> = None;
                    let mut is_sms = false;

                    {
                        let mut state = STATE.lock().await;
                        let tick = state.alive_countdown <= 0;
                        
                        if state.alarm_stack.has_changes() || tick {
                            let bits = state.alarm_stack.export_bits();
                            let str_stack: String<DTMF_PACKET_LENGTH> = bits.iter().collect();
                            state.alive_countdown = ALIVE_PERIOD_MINUTES + 1;

                            if USE_SMS {
                                let time_buf = {
                                    let rtc = RTC.lock().await;
                                    if let Some(ref rtc_ctrl) = *rtc {
                                        let t = rtc_ctrl.get_time();
                                        crate::date_converter::format_gsm_time(&t)
                                    } else {
                                        crate::date_converter::format_gsm_time(&crate::rtc::GsmTime { year:0, month:0, day:0, hour:0, minute:0, second:0 })
                                    }
                                };
                                let mut msg = String::<SIM800_LINE_BUFFER_SIZE>::new();
                                use core::fmt::Write;
                                let _ = write!(msg, "{}{}{}{}{}", SMS_PREFIX, SMS_DIVIDER, str_stack, SMS_DIVIDER, time_buf.as_str());
                                pending_sms = Some(msg);
                                is_sms = true;
                            } else {
                                pending_dtmf = Some(str_stack);
                            }
                        }
                        if !tick { state.alive_countdown -= 1; }
                    }

                    if is_sms {
                        if let Some(msg) = pending_sms { CMD_CHANNEL.send(Command::SendAlarmSms { message: msg }).await; }
                    } else if let Some(dtmf) = pending_dtmf {
                        CMD_CHANNEL.send(Command::CallAlarmWithDtmf { dtmf }).await;
                    }
                }
            },

            // --- CASE 3: WATCHDOG EXPIRED ---
            Either3::Third(_) => {
                info!("Watchdog expired. Resetting outputs.");
                leds.set_system(PowerState::Off);
                leds.set_gsm(PowerState::Off);
                leds.set_error(PowerState::Off);
                leds.set_action(PowerState::Off);
                
                #[cfg(feature = "receiver")]
                relays.set_all(PowerState::Off);

                watchdog_deadline = None;
            }
        }
    }
}

// Helper for Transmitter (Just LEDs)
#[cfg(feature = "transmitter")]
async fn visualize_leds(leds: &mut StatusLeds, alarm_str: &str) {
    visualize_common(alarm_str, |idx, state| {
        leds.set_by_index(idx + 1, state); // Map 0..3 to LED 1..4
    }).await;
}

// Helper for Receiver (Relays + LEDs)
#[cfg(feature = "receiver")]
async fn visualize_relays(relays: &mut AlarmRelays, leds: &mut StatusLeds, alarm_str: &str) {
    visualize_common(alarm_str, |idx, state| {
        relays.set(idx, state);            // Relays 0..3
        leds.set_by_index(idx + 1, state); // LEDs 1..4 (Optional, visual feedback)
    }).await;
}

async fn visualize_common<F>(alarm_str: &str, mut set_output: F)
    where F: FnMut(usize, PowerState) 
    {
        info!("Visualizing: {}", alarm_str);
        
        let mut alarm_chars = ['\0'; ALARMS_MESSAGE_STRING_LENGTH];
        for (i, c) in alarm_str.chars().take(ALARMS_MESSAGE_STRING_LENGTH).enumerate() {
            alarm_chars[i] = c;
        }

        let mut temp_stack = AlarmStack::new();
        temp_stack.import_bits(alarm_chars);
        let matrix = temp_stack.get_stack_view();

        for row in matrix.iter() {
            for (idx, &active) in row.iter().enumerate() {
                set_output(idx, if active { PowerState::On } else { PowerState::Off });
            }
            Timer::after(Duration::from_secs(3)).await;
        }
        
        // Reset all off at end
        /*
        for idx in 0..4 {
            set_output(idx, PowerState::Off);
        }
        */
}

#[embassy_executor::task]
async fn system_monitor_task() {
    loop {
        Timer::after(Duration::from_secs(SYSTEM_MONITOR_PERIOD_HOURS as u64 * 3600)).await;
        CMD_CHANNEL.send(Command::UpdateTime).await;
    }
}